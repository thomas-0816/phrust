use super::*;
use crate::api::{IncludeCache, IncludeLoader};
use crate::experimental::{InlineCacheMode, QuickeningMode, TieringOptions};
use php_ir::{
    FunctionFlags, IrBuilder, IrConstant, IrSpan, Operand, RegId, UnitId,
    instruction::InstructionKind,
};
use php_runtime::api::{ExitStatus, RuntimeDiagnosticPayload, VmCompileDiagnostic};
use std::sync::Arc;

fn test_declaration_origin(kind: DeclarationKind) -> DeclarationOrigin {
    DeclarationOrigin {
        source_path: "dynamic-symbol-test.php".to_owned(),
        line: 1,
        span: IrSpan::default(),
        namespace: None,
        kind,
        load_kind: DeclarationLoadKind::Include,
    }
}

#[test]
fn dynamic_symbol_indexes_preserve_vector_order_and_first_declaration() {
    let mut state = ExecutionState::default();
    let first_unit = CompiledUnit::new(php_ir::IrUnit::new(UnitId::new(1)));
    let equal_but_distinct_unit = CompiledUnit::new(php_ir::IrUnit::new(UnitId::new(1)));

    assert_eq!(state.push_dynamic_unit(first_unit.clone()), 0);
    assert_eq!(state.push_dynamic_unit(first_unit.clone()), 1);
    assert_eq!(
        dynamic_unit_index_for_compiled(&state, &first_unit),
        Some(1)
    );
    assert_eq!(
        dynamic_unit_index_for_compiled(&state, &equal_but_distinct_unit),
        None,
        "identity lookup must not use deep CompiledUnit equality"
    );

    state.push_dynamic_function(DynamicFunctionEntry {
        name: "app\\first".to_owned(),
        unit_index: 0,
        function: php_ir::FunctionId::new(1),
        origin: test_declaration_origin(DeclarationKind::Function),
    });
    state.push_dynamic_function(DynamicFunctionEntry {
        name: "app\\second".to_owned(),
        unit_index: 1,
        function: php_ir::FunctionId::new(2),
        origin: test_declaration_origin(DeclarationKind::Function),
    });
    assert_eq!(
        state
            .dynamic_functions
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["app\\first", "app\\second"]
    );
    assert_eq!(
        dynamic_function_entry_by_normalized_name(&state, "app\\first").map(|entry| entry.function),
        Some(php_ir::FunctionId::new(1))
    );

    state.push_dynamic_constant(DynamicConstantEntry {
        name: "FIRST".to_owned(),
        unit_index: 0,
        value: php_ir::ConstId::new(1),
        origin: test_declaration_origin(DeclarationKind::GlobalConstant),
    });
    state.push_dynamic_constant(DynamicConstantEntry {
        name: "SECOND".to_owned(),
        unit_index: 1,
        value: php_ir::ConstId::new(2),
        origin: test_declaration_origin(DeclarationKind::GlobalConstant),
    });
    assert_eq!(
        state
            .dynamic_constants
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["FIRST", "SECOND"]
    );
    assert_eq!(state.dynamic_constant_index.get("FIRST"), Some(&0));
}

#[test]
fn apcu_builtin_and_callback_share_the_registered_request_slot() {
    let result = execute_source(
        r#"<?php
$key = "__phrust_registered_apcu_slot";
apcu_delete($key);
var_dump(apcu_store($key, "seed"));
var_dump(apcu_entry($key, function ($key) { return "wrong-owner"; }));
var_dump(apcu_delete($key));
"#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nstring(4) \"seed\"\nbool(true)\n"
    );
}

#[test]
fn immutable_unit_validation_is_prepared_once_across_requests() {
    let source = "<?php class PreparedFixture { public function value(): int { return 7; } } echo (new PreparedFixture())->value();";
    let frontend = php_semantics::analyze_source(source);
    assert!(!frontend.has_errors());
    let lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_text: Some(source.to_owned()),
            ..php_ir::LoweringOptions::default()
        },
    );
    let unit = CompiledUnit::new(lowering.unit);
    let vm = Vm::new();

    let first = vm.execute(unit.clone());
    let second = vm.execute(unit.clone());
    assert!(first.status.is_success(), "{:?}", first.status);
    assert!(second.status.is_success(), "{:?}", second.status);
    assert_eq!(first.output.to_string_lossy(), "7");
    assert_eq!(second.output.to_string_lossy(), "7");
    assert_eq!(
        unit.prepared_unit_stats(),
        crate::compiled_unit::PreparedUnitStats {
            ir_verification_runs: 1,
            class_validation_runs: 1,
        }
    );

    let validating_vm = Vm::with_options(VmOptions {
        revalidate_prepared_unit: true,
        ..VmOptions::default()
    });
    let validated = validating_vm.execute(unit.clone());
    assert!(validated.status.is_success(), "{:?}", validated.status);
    assert_eq!(unit.prepared_unit_stats().ir_verification_runs, 1);
    assert_eq!(unit.prepared_unit_stats().class_validation_runs, 1);
}

#[test]
fn pdo_mysql_dsn_parser_accepts_common_tcp_options() {
    let (options, charset) = pdo_mysql_connect_options_from_dsn(
        "mysql:host=127.0.0.1;port=3307;dbname=app;charset=utf8mb4",
        "app_user",
        "app_pass",
    )
    .expect("common PDO MySQL DSN should parse");

    assert!(format!("{options:?}").contains("mysql://"));
    assert_eq!(charset.as_deref(), Some("utf8mb4"));
}

#[test]
fn fileinfo_object_facade_routes_to_runtime_state() {
    let result = execute_source(
        "<?php $f = new finfo(); var_dump(class_exists('finfo'), $f instanceof finfo, $f->set_flags(FILEINFO_MIME_TYPE), finfo_set_flags($f, FILEINFO_MIME_ENCODING), finfo_close($f));",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
}

#[test]
fn fileinfo_constructor_rejects_invalid_magic_database_path() {
    let result =
        execute_source("<?php new finfo(FILEINFO_MIME_TYPE, '/path/to/missing/magic.mgc');");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("E_PHP_VM_FILEINFO_MAGIC")),
        "{:?}",
        result.status
    );
}

#[test]
fn opcache_compile_file_records_only_successful_compiles() {
    let root = std::env::temp_dir().join(format!("phrust-opcache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source_path = root.join("index.php");
    std::fs::write(root.join("valid.php"), "<?php return 42;\n")
        .expect("valid fixture should be written");
    std::fs::write(root.join("invalid.php"), "<?php function {\n")
        .expect("invalid fixture should be written");
    let source = r#"<?php
$valid = __DIR__ . "/valid.php";
$invalid = __DIR__ . "/invalid.php";
var_dump(opcache_compile_file($valid));
var_dump(opcache_is_script_cached($valid));
error_reporting(0);
var_dump(opcache_compile_file($invalid));
var_dump(opcache_is_script_cached($invalid));
"#;

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::new(IncludeCache::new_with_revalidation_interval(
                1,
                std::time::Duration::ZERO,
            ))),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        source_path.display().to_string(),
    );

    let _ = std::fs::remove_dir_all(&root);
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n"
    );
}

#[test]
fn getimagesize_initializes_undefined_image_info_reference() {
    let root =
        std::env::temp_dir().join(format!("phrust-getimagesize-by-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let image_path = root.join("pixel.png");
    std::fs::write(
        &image_path,
        [
            0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde,
        ],
    )
    .expect("fixture should be written");

    let source = format!(
        "<?php $size = getimagesize({:?}, $info); var_dump($size[0], $size[1], is_array($info), array_keys($info));",
        image_path.display().to_string()
    );
    let result = execute_source_with_options_and_path(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::default().with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
        root.join("index.php").display().to_string(),
    );

    let _ = std::fs::remove_dir_all(&root);
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(1)\nint(1)\nbool(true)\narray(0) {\n}\n"
    );
    assert!(
        !result
            .output
            .to_string_lossy()
            .contains("Undefined variable $info"),
        "{}",
        result.output.to_string_lossy()
    );
}

#[test]
fn trace_argument_string_preview_truncates_on_char_boundary() {
    let value = Value::string("12345678901234éXYZ");

    assert_eq!(format_trace_arg(&value), "'12345678901234é...'");
}

#[test]
fn pdo_mysql_dsn_parser_rejects_invalid_port_and_accepts_socket() {
    assert!(
        pdo_mysql_connect_options_from_dsn("mysql:host=db;port=abc", "", "")
            .expect_err("invalid ports should be rejected")
            .contains("invalid MySQL port")
    );

    let (options, charset) = pdo_mysql_connect_options_from_dsn(
        "mysql:unix_socket=/tmp/mysql.sock;dbname=app;charset=utf8mb4",
        "socket_user",
        "socket_pass",
    )
    .expect("unix socket PDO MySQL DSN should parse");
    let rendered = format!("{options:?}");
    assert!(
        rendered.contains("socket=%2Ftmp%2Fmysql.sock"),
        "{rendered}"
    );
    assert!(rendered.contains("/app"), "{rendered}");
    assert_eq!(charset.as_deref(), Some("utf8mb4"));
}

#[test]
fn pdo_mysql_quote_uses_mysql_escaping_rules() {
    assert_eq!(pdo_mysql_quote("a'b\\c\n"), "'a\\'b\\\\c\\n'");
}

#[test]
fn pdo_pgsql_dsn_parser_accepts_common_tcp_options() {
    let options = pdo_pgsql_connect_options_from_dsn(
        "pgsql:host=127.0.0.1;port=5433;dbname=app;sslmode=disable",
        "app_user",
        "app_pass",
    )
    .expect("common PDO PostgreSQL DSN should parse");

    let rendered = format!("{options:?}");
    assert!(rendered.contains("host=127.0.0.1"));
    assert!(rendered.contains("port=5433"));
    assert!(rendered.contains("dbname=app"));
    assert!(rendered.contains("user=app_user"));
}

#[test]
fn pdo_pgsql_dsn_parser_rejects_invalid_port_and_socket_gap() {
    assert!(
        pdo_pgsql_connect_options_from_dsn("pgsql:host=db;port=abc", "", "")
            .expect_err("invalid ports should be rejected")
            .contains("invalid PostgreSQL port")
    );
    assert!(
        pdo_pgsql_connect_options_from_dsn("pgsql:unix_socket=/tmp/.s.PGSQL.5432", "", "")
            .expect_err("unix socket support is not implemented")
            .contains("unix_socket DSNs are not implemented")
    );
}

#[test]
fn pdo_pgsql_quote_uses_postgresql_literal_escaping_rules() {
    assert_eq!(pdo_pgsql_quote("a'b\\c"), "'a''b\\\\c'");
}

#[test]
fn pdo_pgsql_rewrites_positional_placeholders_outside_strings() {
    assert_eq!(
        pdo_pgsql_rewrite_positional_query("SELECT '?' AS literal, ? AS value, ? AS second")
            .expect("placeholders should rewrite"),
        "SELECT '?' AS literal, $1 AS value, $2 AS second"
    );
}

#[test]
fn pdo_pgsql_constructor_failure_raises_pdo_exception() {
    let result = execute_source(
        r#"<?php
try {
    $pdo = new PDO("sqlite::memory:");
    $pdo->__construct("pgsql:host=127.0.0.1;port=abc");
    echo "not-thrown";
} catch (PDOException $e) {
    echo "caught:", strlen($e->getMessage()) > 0 ? "message" : "empty";
}
"#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"caught:message");
}

#[test]
fn by_ref_builtin_direct_temporary_fatal_separates_prior_output() {
    let result = execute_source("<?php echo \"before\\n\"; var_dump(prev(array(1, 2)));");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with(
            "before\n\nFatal error: Uncaught Error: prev(): Argument #1 ($array) could not be passed by reference"
        ),
        "{output}"
    );
}

#[test]
fn error_suppression_wraps_function_call_warnings_and_restores_reporting() {
    let result = execute_source(
        "<?php @preg_match('invalid regex', 'subject'); echo 'suppressed|'; preg_match('invalid regex', 'subject');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output
            .starts_with("suppressed|\nWarning: preg_match(): Delimiter must not be alphanumeric"),
        "{output}"
    );
}

#[test]
fn nested_pcre_callback_redeclaration_uses_php_fatal_output() {
    let result = execute_source(
        "<?php
        function pcre_nested_duplicate() {}
        preg_replace_callback('/a/', function($matches) {
            preg_replace_callback('/x/', function($matches) {
                function pcre_nested_duplicate() {}
                return 'y';
            }, 'x');
            return 'b';
        }, 'a');
        ",
    );

    assert!(!result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with("Fatal error: Cannot redeclare function pcre_nested_duplicate() "),
        "{output}"
    );
    assert!(
        output.contains("(previously declared in /tmp/phrust-test.php:"),
        "{output}"
    );
    assert!(
        output.contains(") in /tmp/phrust-test.php on line "),
        "{output}"
    );
}

#[test]
fn preg_match_pattern_type_errors_are_catchable() {
    let result = execute_source(
        "<?php
        try { preg_match([], 'subject'); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
        try { preg_match(new stdClass(), 'subject'); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
        try { preg_match_all([], 'subject', $matches); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
        try { preg_match_all(new stdClass(), 'subject', $matches); } catch (TypeError $e) { echo $e->getMessage(), \"\\n\"; }
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "preg_match(): Argument #1 ($pattern) must be of type string, array given\n\
preg_match(): Argument #1 ($pattern) must be of type string, stdClass given\n\
preg_match_all(): Argument #1 ($pattern) must be of type string, array given\n\
preg_match_all(): Argument #1 ($pattern) must be of type string, stdClass given\n"
    );
}

#[test]
fn preg_replace_callback_accepts_pattern_arrays() {
    let result = execute_source(
        "<?php
        function wrap_match($matches) {
            return '[' . $matches[0] . ']';
        }
        echo preg_replace_callback(['/x/', '/[0-9]/'], 'wrap_match', 'x1y');
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"[x][1]y");
}

#[test]
fn preg_replace_callback_array_applies_patterns_across_array_subjects() {
    let result = execute_source(
        "<?php
        class PcreTrampoline {
            public function __call($name, $arguments) {
                echo \"callback\\n\";
                return \"'\" . $arguments[0][0] . \"'\";
            }
        }
        $object = new PcreTrampoline();
        var_dump(preg_replace_callback_array([
            '@\\b\\w{1,2}\\b@' => [$object, 'missing'],
            '~\\A.~' => [$object, 'missing'],
        ], ['a b3 bcd', 'v' => 'aksfjk', 12 => 'aa bb', ['xyz']]));
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    let warning = output
        .find("Warning: Array to string conversion")
        .expect("nested array subject should warn");
    assert_eq!(
        output[..warning].matches("callback\n").count(),
        4,
        "{output}"
    );
    assert_eq!(
        output[warning..].matches("callback\n").count(),
        4,
        "{output}"
    );
    assert!(output.contains("string(14) \"'''a' 'b3' bcd\""), "{output}");
    assert!(output.contains("string(7) \"'A'rray\""), "{output}");
}

#[test]
fn preg_replace_callback_array_type_error_uncaught_uses_php_fatal_output() {
    let result = execute_source("<?php preg_replace_callback_array([0 => 'strlen'], 'a');");

    assert!(!result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains(
            "\nFatal error: Uncaught TypeError: preg_replace_callback_array(): Argument #1 ($pattern) must contain only string patterns as keys in "
        ),
        "{output}"
    );
    assert!(
        output.contains(": preg_replace_callback_array(Array, 'a')"),
        "{output}"
    );
    assert!(
        output.contains("  thrown in /tmp/phrust-test.php on line "),
        "{output}"
    );
}

#[test]
fn preg_replace_callback_array_type_errors_are_catchable() {
    let result = execute_source(
        "<?php
        try {
            preg_replace_callback_array([0 => 'strlen'], 'a');
        } catch (TypeError $e) {
            echo $e->getMessage(), \"\\n\";
        }
        try {
            preg_replace_callback_array(['/a/' => 'missing_callback'], 'a');
        } catch (TypeError $e) {
            echo $e->getMessage(), \"\\n\";
        }
        $count = '';
        try {
            preg_replace_callback_array(['xx' => 'missing_callback'], [], -1, $count);
        } catch (TypeError $e) {
            echo $e->getMessage(), \"\\n\";
        }
        var_dump($count);
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "preg_replace_callback_array(): Argument #1 ($pattern) must contain only string patterns as keys\n\
preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid callbacks\n\
preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid callbacks\n\
string(0) \"\"\n"
    );
}

#[test]
fn preg_replace_callback_casts_nested_array_subjects_with_warning() {
    let result = execute_source(
        "<?php
        function quote_match($matches) {
            return \"'\" . $matches[0] . \"'\";
        }
        var_dump(preg_replace_callback('~\\A.~', 'quote_match', array(array('xyz'))));
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Warning: Array to string conversion in "),
        "{output}"
    );
    assert!(output.contains("string(7) \"'A'rray\""), "{output}");
}

#[test]
fn preg_replace_callback_warns_for_remaining_array_subject_before_interrupting() {
    let result = execute_source(
        "<?php
        class PcreThrower {
            public function __call($name, $arguments) {
                echo \"callback\\n\";
                throw new Exception('boom');
            }
        }
        $thrower = new PcreThrower();
        try {
            preg_replace_callback('/a/', [$thrower, 'missing'], ['a', ['nested']]);
        } catch (Throwable $e) {
            echo $e::class, ': ', $e->getMessage(), \"\\n\";
        }
        try {
            preg_replace_callback([new stdClass()], 'strlen', ['a', ['nested']]);
        } catch (Throwable $e) {
            echo $e::class, ': ', $e->getMessage(), \"\\n\";
        }
        try {
            preg_replace_callback([new stdClass()], 'strlen', new stdClass());
        } catch (Throwable $e) {
            echo $e::class, ': ', $e->getMessage(), \"\\n\";
        }
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    let first_warning = output
        .find("Warning: Array to string conversion")
        .expect("callback exception should emit remaining array warning");
    let exception = output
        .find("Exception: boom")
        .expect("callback exception should be caught");
    assert!(first_warning < exception, "{output}");
    let second_warning = output[first_warning + 1..]
        .find("Warning: Array to string conversion")
        .map(|index| first_warning + 1 + index)
        .expect("pattern conversion error should emit array warning");
    let pattern_error = output
        .find("Error: Object of class stdClass could not be converted to string")
        .expect("pattern object conversion should be caught");
    assert!(second_warning < pattern_error, "{output}");
    assert!(
        output.ends_with(
            "TypeError: preg_replace_callback(): Argument #3 ($subject) must be of type array|string, stdClass given\n"
        ),
        "{output}"
    );
}

#[test]
fn runtime_function_redeclaration_uses_php_fatal_output() {
    let result = execute_source(
        "<?php
        function pcre_runtime_duplicate() {}
        preg_replace_callback('/a/', function($matches) {
            function pcre_runtime_duplicate() {}
            return 'b';
        }, 'a');
        ",
    );

    assert!(!result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains(
            "Fatal error: Cannot redeclare function pcre_runtime_duplicate() (previously declared in /tmp/phrust-test.php:"
        ),
        "{output}"
    );
    assert!(
        output.contains(") in /tmp/phrust-test.php on line "),
        "{output}"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_VM_FUNCTION_REDECLARATION"),
        "{:#?}",
        result.diagnostics
    );
}

#[test]
fn user_stream_wrapper_close_runs_at_shutdown_and_can_use_pcre() {
    let result = execute_source(
        r#"<?php
        class wrapper {
            public function stream_open($path, $mode, $options, &$opened_path) { return true; }
            public function stream_close() {
                echo "Close\n";
                preg_replace('/pattern/', 'replace', 'subject');
                preg_match('/(4)?(2)?\d/', '23456', $matches, PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL);
                preg_match('/(4)?(2)?\d/', '23456', $matches, PREG_OFFSET_CAPTURE);
            }
        }

        echo stream_wrapper_register('wrapper', 'wrapper') ? "registered\n" : "failed\n";
        $handle = fopen('wrapper://', 'rb');
        "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"registered\nClose\n");
}

fn property_fetch_profile<'a>(
    counters: &'a VmCounters,
    property: &str,
) -> &'a crate::counters::PropertyFetchProfile {
    counters
        .property_fetch_profiles
        .values()
        .find(|profile| profile.property == property)
        .unwrap_or_else(|| panic!("missing property fetch profile for ${property}"))
}

fn method_call_profile<'a>(
    counters: &'a VmCounters,
    method: &str,
) -> &'a crate::counters::MethodCallProfile {
    counters
        .method_call_profiles
        .values()
        .find(|profile| profile.method == method)
        .unwrap_or_else(|| panic!("missing method call profile for {method}()"))
}

#[test]
fn vm_core_returns_null_from_manual_ir() {
    let unit = manual_return_unit(IrConstant::Null);
    let result = Vm::new().execute(unit);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.return_value, Some(Value::Null));
    assert_eq!(result.output.as_bytes(), b"");
}

#[test]
fn vm_core_echoes_string_from_manual_ir() {
    let unit = manual_echo_unit(IrConstant::String("hello".to_string()));
    let result = Vm::new().execute(unit);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.return_value, Some(Value::Null));
    assert_eq!(result.output.as_bytes(), b"hello");
}

#[test]
fn vm_core_echoes_int_from_manual_ir() {
    let unit = manual_echo_unit(IrConstant::Int(123));
    let result = Vm::new().execute(unit);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"123");
}

#[test]
fn vm_core_bad_register_is_controlled_when_verifier_is_disabled() {
    let mut unit = manual_return_unit(IrConstant::Null);
    unit.functions[0].blocks[0]
        .instructions
        .push(php_ir::Instruction {
            id: php_ir::InstrId::new(1),
            span: IrSpan::new(php_ir::FileId::new(0), 0, 0),
            kind: InstructionKind::Move {
                dst: RegId::new(0),
                src: Operand::Register(RegId::new(99)),
            },
        });
    let vm = Vm::with_options(VmOptions {
        verify_ir: false,
        ..VmOptions::default()
    });

    let result = vm.execute(unit);

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(result.status.message(), Some("invalid register r99"));
}

#[test]
fn expressions_execute_arithmetic_concat_unary_and_comparisons() {
    let result = execute_source(
        "<?php echo 1 + 2 * 3, \"|\", \"a\" . \"b\", \"|\", !false, \"|\", 2 <=> 3;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7|ab|1|-1");
}

#[test]
fn expressions_exact_division_and_integer_power_preserve_int_results() {
    let result = execute_source("<?php var_dump(8 / 4, 7 / 4, 2 ** 3);");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(2)\nfloat(1.75)\nint(8)\n"
    );
}

#[test]
fn expressions_power_zero_negative_exponent_emits_deprecation() {
    let result = execute_source("<?php var_dump(0 ** -1);");

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Deprecated: Power of base 0 and negative exponent is deprecated"));
    assert!(output.contains("float(INF)"));
}

#[test]
fn expressions_execute_bitwise_and_assignment_operators() {
    let result = execute_source(
        "<?php $x = 6; $x &= 3; echo $x, '|', (6 | 3), '|', (6 ^ 3), '|', (8 << 1), '|', (8 >> 1), '|'; echo bin2hex('12' & '3'), '|', bin2hex('12' | '3'), '|', bin2hex('12' ^ '3');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2|7|5|16|4|31|3332|02");
}

#[test]
fn expressions_list_assignment_holes_preserve_numeric_positions() {
    let result = execute_source(
        r#"<?php
            $next_token = array("block-opener", "name", array("style" => array()), 12, 34);
            list($token_type, , $attrs, $start_offset, $token_length) = $next_token;
            echo $token_type, "|", gettype($attrs), "|", $start_offset, "|", $token_length;
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"block-opener|array|12|34");
}

#[test]
fn expressions_execute_casts_and_truthiness() {
    let result =
        execute_source("<?php echo (int) \"12\", \"|\", (string) true, \"|\", (bool) \"0\";");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12|1|");
}

#[test]
fn expressions_object_casts_create_std_class_values() {
    let result = execute_source(
        "<?php
            var_export((object) array(1, 3, 'foo' => 'bar'));
            echo \"\\n---\\n\";
            var_export((object) 42);
            echo \"\\n---\\n\";
            var_export((object) null);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "(object) array(\n   '0' => 1,\n   '1' => 3,\n   'foo' => 'bar',\n)\n---\n(object) array(\n   'scalar' => 42,\n)\n---\n(object) array(\n)"
    );
}

#[test]
fn expressions_object_cast_preserves_nested_object_identity() {
    let result = execute_source(
        "<?php
            $array = ['x' => 1, 'child' => new stdClass()];
            $object = (object) $array;
            var_dump($object);
            echo json_encode($object), '|', json_last_error();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = normalize_object_debug_ids(&result.output.to_string_lossy());
    assert_eq!(
        output,
        "object(stdClass)#%d (2) {\n  [\"x\"]=>\n  int(1)\n  [\"child\"]=>\n  object(stdClass)#%d (0) {\n  }\n}\n{\"x\":1,\"child\":{}}|0"
    );
}

#[test]
fn expressions_array_casts_match_php_shapes() {
    let result = execute_source(
        "<?php
            var_dump((array) null, (array) \"type1\", (array) 10, (array) 12.34);
            $object = new stdClass();
            $object->a = 1;
            $object->b = \"two\";
            var_dump((array) $object);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array(0) {\n}\narray(1) {\n  [0]=>\n  string(5) \"type1\"\n}\narray(1) {\n  [0]=>\n  int(10)\n}\narray(1) {\n  [0]=>\n  float(12.34)\n}\narray(2) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  string(3) \"two\"\n}\n"
    );
}

#[test]
fn expressions_array_casts_mangle_declared_object_property_keys() {
    let result = execute_source(
        "<?php
            class foo {
                private $private = 'private';
                protected $protected = 'protected';
                public $public = 'public';
            }
            var_export((array) new foo);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array (\n  '' . \"\\0\" . 'foo' . \"\\0\" . 'private' => 'private',\n  '' . \"\\0\" . '*' . \"\\0\" . 'protected' => 'protected',\n  'public' => 'public',\n)"
    );
}

#[test]
fn expressions_object_numeric_cast_warns_and_returns_one() {
    let result =
        execute_source("<?php class Box {} $box = new Box; var_dump((int) $box, (float) $box);");

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Warning: Object of class Box could not be converted to int"));
    assert!(output.contains("Warning: Object of class Box could not be converted to float"));
    assert!(output.contains("int(1)"));
    assert!(output.contains("float(1)"));
}

#[test]
fn curl_exec_invokes_header_and_write_callbacks() {
    let _network_override = php_runtime::debug::set_curl_network_tests_override_for_tests(true);
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind local server");
    let port = listener.local_addr().expect("server addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0_u8; 1024];
        let read = std::io::Read::read(&mut stream, &mut request).expect("read request");
        assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /callback"));
        std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nX-Test: yes\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nBODY\r\n0\r\n\r\n",
            )
            .expect("write response");
        stream
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown");
    });

    let result = execute_source(&format!(
        "<?php
            class CurlCollector {{
                public $headers = '';
                public $body = '';
                public function headers($handle, $data) {{
                    $this->headers .= $data;
                    return strlen($data);
                }}
                public function body($handle, $data) {{
                    $this->body .= $data;
                    return strlen($data);
                }}
            }}
            $collector = new CurlCollector();
            $handle = curl_init('http://127.0.0.1:{port}/callback');
            curl_setopt($handle, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($handle, CURLOPT_HEADERFUNCTION, [$collector, 'headers']);
            curl_setopt($handle, CURLOPT_WRITEFUNCTION, [$collector, 'body']);
            $result = curl_exec($handle);
            echo str_starts_with($collector->headers, \"HTTP/1.1 200 OK\\r\\n\") ? 'H' : 'h';
            echo '|', str_contains($collector->headers, \"X-Test: yes\\r\\n\") ? 'X' : 'x';
            echo '|', substr_count($collector->headers, \"HTTP/1.1 200 OK\");
            echo '|', $collector->body;
            echo '|', $result;
            "
    ));
    server.join().expect("server thread");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "H|X|1|BODY|BODY");
}

#[test]
fn expressions_division_by_zero_is_controlled_runtime_error() {
    let result = execute_source("<?php echo 1 / 0;");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(result.status.message(), Some("division by zero"));
}

#[test]
fn expressions_integer_overflow_promotes_to_float() {
    let result =
        execute_source("<?php var_dump(PHP_INT_MAX + 1, PHP_INT_MIN - 1, PHP_INT_MAX * 2);");

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8(result.output.as_bytes().to_vec()).expect("utf8 output");
    assert_eq!(output.matches("float(").count(), 3);
}

#[test]
fn expressions_unary_integer_min_promotes_to_float() {
    let result = execute_source("<?php var_dump(-PHP_INT_MIN);");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(result.output.to_string_lossy().starts_with("float("));
}

#[test]
fn constants_execute_global_const_fetches() {
    let result =
        execute_source("<?php const ANSWER = 42; const WORD = \"ok\"; echo ANSWER, \"|\", WORD;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42|ok");
}

#[test]
fn constants_execute_define_userland_fetches() {
    let result = execute_source(
        "<?php
            var_dump(define('LOCAL_DYNAMIC', 'ok'));
            var_dump(defined('LOCAL_DYNAMIC'));
            echo LOCAL_DYNAMIC, '|', constant('LOCAL_DYNAMIC'), '|';
            var_dump(define('LOCAL_DYNAMIC', 'again'));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
            output.starts_with(
                "bool(true)\nbool(true)\nok|ok|\nWarning: Constant LOCAL_DYNAMIC already defined, this will be an error in PHP 9"
            ),
            "{output}"
        );
    assert!(output.ends_with("bool(false)\n"), "{output}");
}

#[test]
fn constants_execute_builtin_php_version() {
    let result = execute_source("<?php echo PHP_VERSION;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        php_source::reference_php_version().as_bytes()
    );
}

#[test]
fn constants_execute_query_encoding_named_arg() {
    let result = execute_source(
        "<?php echo PHP_QUERY_RFC1738, '|', PHP_QUERY_RFC3986, '|', http_build_query(['a b' => 'c d'], encoding_type: PHP_QUERY_RFC3986);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2|a%20b=c%20d");
}

#[test]
fn http_build_query_encodes_visible_object_properties() {
    let result = execute_source(
        "<?php
            class KeyVal {
                public $public = 'input';
                protected $protected = 'hello';
                private $private = 'world';

                public function scoped() {
                    return http_build_query($this);
                }
            }

            $object = new KeyVal();
            var_dump(http_build_query($object));
            var_dump($object->scoped());

            $nested = new KeyVal();
            $object->public = $nested;
            var_dump(http_build_query($object));

            $object->public = $object;
            var_dump(http_build_query($object));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"string(12) \"public=input\"\nstring(42) \"public=input&protected=hello&private=world\"\nstring(24) \"public%5Bpublic%5D=input\"\nstring(0) \"\"\n"
        );
}

#[test]
fn constants_execute_namespaced_global_fallback_for_lexical_fetch() {
    let result = execute_source(
        "<?php namespace WpOrg\\Requests\\Transport; echo CURLOPT_HEADER, '|', \\CURLOPT_HEADER, '|', CURL_HTTP_VERSION_1_1, '|', CURLOPT_HEADERFUNCTION;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42|42|2|20079");
}

#[test]
fn constants_constant_builtin_keeps_namespaced_lookup_exact() {
    let result = execute_source(
        "<?php namespace WpOrg\\Requests\\Transport; echo constant('WpOrg\\\\Requests\\\\Transport\\\\CURLOPT_HEADER');",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_CONSTANT"
    );
}

#[test]
fn constants_execute_internal_enum_cases_by_dynamic_name() {
    let result = execute_source(
        "<?php echo defined('RoundingMode::AwayFromZero') ? 'D|' : 'd|'; echo round(1.2, 0, constant('RoundingMode::AwayFromZero'));",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"D|2");
}

#[test]
fn enums_execute_unit_cases_static_method() {
    let result = execute_source(
        "<?php enum Status { case Draft; case Published; } foreach (Status::cases() as $case) { echo $case->name, \"\\n\"; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"Draft\nPublished\n");
}

#[test]
fn inline_html_after_close_tag_is_emitted() {
    let result = execute_source("<?php echo \"before\\n\"; ?>\n\nDONE\n");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"before\n\nDONE\n");
}

#[test]
fn constants_execute_operator_predefined_values() {
    let result = execute_source(
        "<?php echo PHP_INT_SIZE, '|', PHP_INT_MAX, '|', PHP_INT_MIN, '|'; var_dump(INF, NAN);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8(result.output.as_bytes().to_vec()).expect("utf8 output");
    assert!(output.starts_with(&format!(
        "{}|{}|{}|",
        std::mem::size_of::<isize>(),
        isize::MAX,
        isize::MIN
    )));
    assert!(output.contains("float(INF)"));
    assert!(output.contains("float(NAN)"));
}

#[test]
fn constants_report_undefined_constant() {
    let result = execute_source("<?php echo MISSING_CONSTANT;");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_CONSTANT"
    );
}

#[test]
fn class_constant_initializer_errors_use_initializer_location() {
    let result = execute_source(
        "<?php
class C
{
    const c1 = D::hello;
}

$a = new C();
",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Fatal error: Uncaught Error: Class \"D\" not found in "),
        "{output}"
    );
    assert!(output.contains("\nStack trace:\n#0 "), "{output}");
    assert!(
        output.contains("): [constant expression]()\n#1 {main}"),
        "{output}"
    );
    assert!(output.contains("  thrown in "), "{output}");
    assert!(output.contains(" on line "), "{output}");
}

#[test]
fn missing_parent_class_declaration_error_is_catchable() {
    let result = execute_source(
        "<?php
try {
    class A extends B {}
} catch (Error $e) {
    var_dump(class_exists('A'));
    var_dump(class_exists('B'));
    throw $e;
}
",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with("bool(false)\nbool(false)\n"), "{output}");
    assert!(
        output.contains("Fatal error: Uncaught Error: Class \""),
        "{output}"
    );
    assert!(output.contains("\" not found in "), "{output}");
}

#[test]
fn datetimeinterface_user_implementation_fails_at_runtime_declaration() {
    let result = execute_source(
        "<?php
class AllowedDateTimeChild extends DateTime implements DateTimeInterface {}
echo \"before\\n\";
class BadDateTimeInterfaceImplementation implements DateTimeInterface {}
",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with("before\n\nFatal error: "), "{output}");
    assert!(
        output.contains("DateTimeInterface can't be implemented by user classes"),
        "{output}"
    );
    assert!(!output.contains("Stack trace:"), "{output}");
}

#[test]
fn symbol_introspection_core_functions_use_runtime_symbols() {
    let result = execute_source(
            "<?php
            const LOCAL_CONST = 41;
            function local_fn() {}
            interface I {}
            class A {
                public $x;
                public function m() {}
                public function called() { return get_called_class(); }
                public function closureCalled() { $f = function () { return get_called_class(); }; return $f(); }
                public static function staticCalled() { return get_called_class(); }
            }
            class B extends A implements I {}
            enum E { case One; }
            $b = new B();
            $b->dyn = 1;
            echo defined('LOCAL_CONST') ? 'D' : 'd';
            echo '|', constant('LOCAL_CONST');
            echo '|', function_exists('LOCAL_FN') ? 'F' : 'f';
            echo '|', class_exists('b', false) ? 'C' : 'c';
            echo '|', interface_exists('i', false) ? 'I' : 'i';
            echo '|', enum_exists('e', false) ? 'E' : 'e';
            echo '|', method_exists('B', 'M') ? 'M' : 'm';
            echo '|', property_exists('B', 'x') ? 'P' : 'p';
            echo '|', property_exists($b, 'dyn') ? 'Y' : 'y';
            echo '|', is_subclass_of('B', 'A') ? 'S' : 's';
            echo '|', is_subclass_of('B', 'A', false) ? 'bad' : 'N';
            echo '|', $b->called();
            echo '|', $b->closureCalled();
            echo '|', B::staticCalled();
            echo '|', get_class($b);
            echo '|', get_parent_class('B');
            echo '|', in_array('B', get_declared_classes(), true) ? 'DC' : 'dc';
            echo '|', in_array('I', get_declared_interfaces(), true) ? 'DI' : 'di';
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "D|41|F|C|I|E|M|P|Y|S|N|B|B|B|B|A|DC|DI"
    );
}

#[test]
fn symbol_introspection_hides_arginfo_only_internal_classes() {
    let result = execute_source(
        "<?php
            echo class_exists('SessionHandler', false) ? 'class' : 'no-class';
            echo '|', interface_exists('SessionHandlerInterface', false) ? 'iface' : 'no-iface';
            echo '|', class_exists('stdClass', false) ? 'stdclass' : 'no-stdclass';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "no-class|no-iface|stdclass"
    );
}

#[test]
fn symbol_introspection_exposes_bounded_mbstring_mvp() {
    let result = execute_source(
        "<?php
            echo extension_loaded('mbstring') ? 'loaded' : 'missing';
            foreach ([
                'mb_check_encoding',
                'mb_convert_encoding',
                'mb_strlen',
                'mb_substr',
                'mb_strtolower',
                'mb_strtoupper',
                'mb_detect_encoding',
                'mb_encoding_aliases',
                'mb_strpos',
                'mb_list_encodings',
                'mb_substitute_character',
            ] as $name) {
                echo '|', $name, ':', function_exists($name) ? 'yes' : 'no';
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "loaded|mb_check_encoding:yes|mb_convert_encoding:yes|mb_strlen:yes|mb_substr:yes|mb_strtolower:yes|mb_strtoupper:yes|mb_detect_encoding:yes|mb_encoding_aliases:yes|mb_strpos:yes|mb_list_encodings:yes|mb_substitute_character:yes"
    );
}

#[test]
fn mbstring_substitute_character_state_persists_across_vm_builtin_calls() {
    let result = execute_source(
        "<?php
            echo mb_substitute_character();
            echo '|', mb_substitute_character('none') ? 'set' : 'fail';
            echo '|', mb_substitute_character();
            echo '|', mb_substitute_character(63) ? 'set' : 'fail';
            echo '|', mb_substitute_character();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "63|set|none|set|63");
}

#[test]
fn nested_function_exists_polyfill_guard_declares_one_branch() {
    let result = execute_source(
        "<?php
            if ( ! function_exists( 'utf8_encode' ) ) :
                if ( extension_loaded( 'mbstring' ) ) :
                    function utf8_encode( $value ) { return 'mb'; }
                else :
                    function utf8_encode( $value ) { return 'fallback'; }
                endif;
            endif;
            echo function_exists( 'utf8_encode' ) ? 'declared' : 'missing';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "declared");
}

#[test]
fn object_and_class_handling_functions_respect_visibility() {
    let result = execute_source(
        "<?php
            class Box {
                public $pub = 'P';
                protected $prot = 'R';
                private $priv = 'V';
                public function objectVars() {
                    $vars = get_object_vars($this);
                    return $vars['pub'] . $vars['prot'] . $vars['priv'];
                }
                public function mangledVars() {
                    $vars = get_mangled_object_vars($this);
                    return $vars[\"\0*\0prot\"] . $vars[\"\0Box\0priv\"];
                }
                public static function classVars() {
                    $vars = get_class_vars('Box');
                    return $vars['pub'] . $vars['prot'] . $vars['priv'];
                }
                public function hidden() { return 'hidden'; }
                protected function protMethod() {}
                private function privMethod() {}
            }
            $box = new Box();
            $outside = get_object_vars($box);
            $methods = get_class_methods('Box');
            $classVars = get_class_vars('Box');
            echo $outside['pub'];
            echo '|', array_key_exists('prot', $outside) ? 'bad' : 'no-prot';
            echo '|', $box->objectVars();
            echo '|', $box->mangledVars();
            echo '|', Box::classVars();
            echo '|', in_array('hidden', $methods, true) ? 'method' : 'missing';
            echo '|', in_array('protMethod', $methods, true) ? 'bad' : 'no-prot-method';
            echo '|', array_key_exists('priv', $classVars) ? 'bad' : 'no-priv-var';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "P|no-prot|PRV|RV|PRV|method|no-prot-method|no-priv-var"
    );
}

#[test]
fn callable_and_function_context_builtins_use_vm_call_path() {
    let result = execute_source(
            "<?php
            function join_args($a, $b = 'D') {
                return $a . $b . ':' . func_num_args() . ':' . func_get_arg(0) . ':' . count(func_get_args());
            }
            function named_args($a, $b) { return $a . $b; }
            class CallTarget {
                public static function target($value) { return 'S' . $value; }
                public static function forward($value) {
                    return forward_static_call(['CallTarget', 'target'], $value);
                }
            }
            echo call_user_func('join_args', 'A', 'B');
            echo '|', call_user_func_array('named_args', ['b' => 'B', 'a' => 'A']);
            echo '|', call_user_func(['CallTarget', 'target'], 'X');
            echo '|', CallTarget::forward('Y');
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "AB:2:A:2|AB|SX|SY");
}

#[test]
fn call_user_func_allows_protected_method_from_class_scope() {
    let result = execute_source(
        "<?php
            class ScopedCallbackTarget {
                protected string $value = '';
                public function __set($name, $value): void {
                    call_user_func(array($this, 'set_' . $name), $value);
                }
                public function value(): string { return $this->value; }
                protected function set_value($value): void { $this->value = $value; }
            }
            $target = new ScopedCallbackTarget();
            $target->value = 'ok';
            echo $target->value();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "ok");
}

#[test]
fn call_user_func_method_can_call_inherited_protected_parent_method() {
    let result = execute_source(
        "<?php
            class RestBaseProbe {
                protected function add_additional_fields_schema($schema) {
                    return $schema . ':parent';
                }
            }
            class RestPostsProbe extends RestBaseProbe {
                public function get_item_schema() {
                    return $this->add_additional_fields_schema('schema');
                }
            }
            $controller = new RestPostsProbe();
            echo call_user_func(array($controller, 'get_item_schema'));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "schema:parent");
}

#[test]
fn call_user_func_route_schema_callback_keeps_child_scope_for_protected_parent_method() {
    let result = execute_source(
        "<?php
            class RestRouteBaseProbe {
                protected function add_additional_fields_schema($schema) {
                    return $schema . ':parent';
                }
            }
            class RestRoutePostsProbe extends RestRouteBaseProbe {
                private $schema = '';
                public function get_item_schema() {
                    if ($this->schema) {
                        return $this->add_additional_fields_schema($this->schema);
                    }
                    $this->schema = 'schema';
                    return $this->add_additional_fields_schema($this->schema);
                }
                public function get_public_item_schema() {
                    return $this->get_item_schema();
                }
            }
            $controller = new RestRoutePostsProbe();
            $routes = array(
                '/wp/v2/posts' => array(
                    'schema' => array($controller, 'get_public_item_schema'),
                ),
            );
            $options = $routes['/wp/v2/posts'];
            echo call_user_func($options['schema']);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "schema:parent");
}

#[test]
fn symbol_introspection_respects_autoload_flag() {
    let result = execute_source(
        "<?php
            function mark_symbol($name) { echo 'autoload:', $name, '|'; }
            spl_autoload_register('mark_symbol');
            echo class_exists('MissingSymbol', false) ? 'T' : 'F';
            echo '|';
            echo class_exists('MissingSymbol', true) ? 'T' : 'F';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "F|autoload:MissingSymbol|F"
    );
}

#[test]
fn spl_autoload_stack_preserves_order_lists_and_unregisters_callbacks() {
    let result = execute_source(
        "<?php
            function first_loader($name) { echo 'first:', $name, '|'; }
            function second_loader($name) { echo 'second:', $name, '|'; }
            spl_autoload_register('first_loader');
            spl_autoload_register('second_loader');
            echo count(spl_autoload_functions()), '|';
            spl_autoload_call('MissingClass');
            echo '|';
            echo spl_autoload_unregister('first_loader') ? 'removed' : 'missing';
            echo '|', count(spl_autoload_functions()), '|';
            spl_autoload_call('OtherClass');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "2|first:MissingClass|second:MissingClass||removed|1|second:OtherClass|"
    );
}

#[test]
fn spl_autoload_object_callback_retains_object_until_shutdown() {
    let result = execute_source(
        "<?php
            class AutoloadHolder {
                public $var = 1;
                public function autoload($name) { echo 'var:', $this->var, '|'; }
                public function __destruct() { echo '__destruct__'; }
            }
            $holder = new AutoloadHolder();
            $holder->var = 2;
            spl_autoload_register([$holder, 'autoload']);
            unset($holder);
            var_dump(class_exists('MissingAutoloadLifetime', true));
            echo 'done|';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "var:2|bool(false)\ndone|__destruct__"
    );
}

#[test]
fn spl_autoload_register_prepend_orders_callback_before_existing_stack() {
    let result = execute_source(
            "<?php
            function first_loader($name) { echo 'first:', $name, '|'; }
            function second_loader($name) { echo 'second:', $name, '|'; }
            function declaring_loader($name) { echo 'declaring:', $name, '|'; eval('class ' . $name . '{}'); }
            spl_autoload_register('first_loader');
            spl_autoload_register('second_loader', true, true);
            spl_autoload_register('declaring_loader');
            new PrependedAutoloadTarget;
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "second:PrependedAutoloadTarget|first:PrependedAutoloadTarget|declaring:PrependedAutoloadTarget|"
    );
}

#[test]
fn spl_autoload_exception_from_new_is_catchable() {
    let result = execute_source(
        "<?php
            function throwing_loader($name) {
                echo 'autoload:', $name, '|';
                throw new Exception('first');
            }
            spl_autoload_register('throwing_loader');
            try {
                new MissingAutoloadTarget;
            } catch (Exception $e) {
                echo $e->getMessage();
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "autoload:MissingAutoloadTarget|first"
    );
}

#[test]
fn spl_autoload_skips_invalid_dynamic_class_names() {
    let result = execute_source(
        "<?php
            spl_autoload_register(function ($name) {
                echo $name, \"\\n\";
            });
            $class = '../BUG';
            new $class;
            ",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Fatal error: Uncaught Error: Class \"../BUG\" not found in "),
        "{output}"
    );
    assert!(!output.starts_with("../BUG\n"), "{output}");
}

#[test]
fn autoload_static_constant_property_and_method_access() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-static-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Target.php"),
        "<?php
            class StaticAutoloadTarget {
                public const VALUE = 'const';
                public static string $prop = 'prop';
                public static function method(): string { return 'method'; }
            }
            ",
    )
    .expect("autoload target should be written");
    let source = "<?php
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                require __DIR__ . '/Target.php';
            });
            echo StaticAutoloadTarget::VALUE, '|';
            echo StaticAutoloadTarget::$prop, '|';
            echo StaticAutoloadTarget::method(), '|';
            echo isset(StaticAutoloadTarget::$prop) ? 'isset' : 'missing';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "load:StaticAutoloadTarget|const|prop|method|isset"
    );
}

#[test]
fn autoload_static_method_uses_resolved_import_name() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-static-import-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("InputValidator.php"),
        "<?php
            namespace WpOrg\\Requests\\Utility;
            final class InputValidator {
                public static function is_string_or_stringable($value) {
                    return true;
                }
            }
            ",
    )
    .expect("autoload target should be written");
    let source = "<?php
            namespace WpOrg\\Requests;
            use WpOrg\\Requests\\Utility\\InputValidator;
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                if ($class === 'WpOrg\\\\Requests\\\\Utility\\\\InputValidator') {
                    require __DIR__ . '/InputValidator.php';
                }
            });
            echo InputValidator::is_string_or_stringable('ok') ? 'ok' : 'bad';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "load:WpOrg\\Requests\\Utility\\InputValidator|ok"
    );
}

#[test]
fn autoload_parent_class_before_static_child_materialization() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-parent-import-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Base.php"),
        "<?php
            namespace Vendor;
            abstract class Base {}
            ",
    )
    .expect("parent class target should be written");
    let source = "<?php
            use Vendor\\Base;
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                if ($class === 'Vendor\\\\Base') {
                    require __DIR__ . '/Base.php';
                }
            });
            class Child extends Base {
                public static function init(): string { return 'ok'; }
            }
            echo Child::init();
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("load:Vendor\\Base|"), "{output}");
    assert!(output.contains("ok"), "{output}");
    assert!(!output.contains("load:vendor\\base|"), "{output}");
}

#[test]
fn autoloaded_parent_protected_arrayaccess_storage_round_trips() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-arrayaccess-parent-storage-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("BaseDictionary.php"),
        "<?php
            namespace Vendor;
            class BaseDictionary implements \\ArrayAccess {
                protected array $data = [];
                public function offsetExists($offset): bool {
                    return isset($this->data[strtolower($offset)]);
                }
                public function offsetGet($offset): mixed {
                    return $this->data[strtolower($offset)] ?? null;
                }
                public function offsetSet($offset, $value): void {
                    $offset = strtolower($offset);
                    if (!isset($this->data[$offset])) {
                        $this->data[$offset] = [];
                    }
                    $this->data[$offset][] = $value;
                }
                public function offsetUnset($offset): void {
                    unset($this->data[strtolower($offset)]);
                }
            }
            ",
    )
    .expect("parent class target should be written");
    std::fs::write(
        root.join("Headers.php"),
        "<?php
            namespace Vendor;
            class Headers extends BaseDictionary {}
            ",
    )
    .expect("child class target should be written");
    let source = "<?php
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                if ($class === 'Vendor\\\\Headers') {
                    require __DIR__ . '/Headers.php';
                }
                if ($class === 'Vendor\\\\BaseDictionary') {
                    require __DIR__ . '/BaseDictionary.php';
                }
            });
            $headers = new Vendor\\Headers();
            $headers['Content-Type'] = 'text/html';
            echo $headers['content-type'][0] ?? 'missing';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "load:Vendor\\Headers|load:Vendor\\BaseDictionary|text/html"
    );
}

#[test]
fn included_child_parent_static_call_resolves_in_dynamic_state() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-parent-static-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Base.php"),
        "<?php
            class IncludedParent {
                public function value(): string { return 'parent'; }
            }
            ",
    )
    .expect("parent class should be written");
    std::fs::write(
        root.join("Child.php"),
        "<?php
            class IncludedChild extends IncludedParent {
                public function value(): string { return parent::value() . '|child'; }
            }
            ",
    )
    .expect("child class should be written");
    let source = "<?php
            require __DIR__ . '/Base.php';
            require __DIR__ . '/Child.php';
            echo (new IncludedChild())->value();
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "parent|child");
}

#[test]
fn included_parent_static_factory_instantiates_later_child() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-parent-static-factory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Base.php"),
        "<?php
            #[AllowDynamicProperties]
            abstract class IncludedFactoryBase {
                protected function __construct(public int $user_id) {}
                final public static function get_instance(int $user_id) {
                    $manager = 'IncludedFactoryChild';
                    return new $manager($user_id);
                }
                final public function verify(string $token): string {
                    return 'base:' . $this->user_id . ':' . $token;
                }
            }
            ",
    )
    .expect("parent class should be written");
    std::fs::write(
        root.join("Child.php"),
        "<?php
            class IncludedFactoryChild extends IncludedFactoryBase {
            }
            ",
    )
    .expect("child class should be written");
    let source = "<?php
            require __DIR__ . '/Base.php';
            require __DIR__ . '/Child.php';
            echo IncludedFactoryBase::get_instance(7)->verify('token');
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "base:7:token");
}

#[test]
fn included_magic_property_methods_handle_inaccessible_declared_properties() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-magic-property-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Box.php"),
        "<?php
            class IncludedMagicBox {
                protected $hidden = 'seed';
                public function __get($name) { return $this->$name; }
                public function __set($name, $value): void { $this->$name = $value; }
            }
            ",
    )
    .expect("magic class should be written");
    let source = "<?php
            require __DIR__ . '/Box.php';
            class IncludedMagicReader {
                public static function run() {
                    $box = new IncludedMagicBox();
                    echo $box->hidden, '|';
                    $box->hidden = 'changed';
                    echo $box->hidden;
                }
            }
            IncludedMagicReader::run();
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "seed|changed");
}

#[test]
fn autoload_imported_interface_dependency_uses_resolved_name() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-interface-import-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("HookManager.php"),
        "<?php
            namespace WpOrg\\Requests;
            interface HookManager {}
            ",
    )
    .expect("interface target should be written");
    std::fs::write(
        root.join("Hooks.php"),
        "<?php
            namespace WpOrg\\Requests;
            use WpOrg\\Requests\\HookManager;
            class Hooks implements HookManager {}
            ",
    )
    .expect("class target should be written");
    let source = "<?php
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                if ($class === 'WpOrg\\\\Requests\\\\Hooks') {
                    require __DIR__ . '/Hooks.php';
                } elseif ($class === 'WpOrg\\\\Requests\\\\HookManager') {
                    require __DIR__ . '/HookManager.php';
                }
            });
            echo class_exists('WpOrg\\\\Requests\\\\Hooks') ? 'ok' : 'bad';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "load:WpOrg\\Requests\\Hooks|load:WpOrg\\Requests\\HookManager|ok"
    );
}

#[test]
fn autoload_global_imported_interface_dependency_uses_resolved_name() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-global-interface-import-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Dependency.php"),
        "<?php
            namespace Vendor\\Package;
            interface Dependency {}
            ",
    )
    .expect("interface target should be written");
    std::fs::write(
        root.join("Implementation.php"),
        "<?php
            use Vendor\\Package\\Dependency;

            class Implementation implements Dependency {}
            ",
    )
    .expect("class target should be written");
    let source = "<?php
            spl_autoload_register(function ($class) {
                echo 'load:', $class, '|';
                if ($class === 'Implementation') {
                    require __DIR__ . '/Implementation.php';
                } elseif ($class === 'Vendor\\\\Package\\\\Dependency') {
                    require __DIR__ . '/Dependency.php';
                }
            });
            echo class_exists('Implementation') ? 'ok' : 'bad';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "load:Implementation|load:Vendor\\Package\\Dependency|ok"
    );
}

#[test]
fn autoload_static_method_from_dynamic_include_uses_declaring_unit() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-static-autoload-owner-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("OwnerTarget.php"),
        "<?php
            class StaticAutoloadOwnerTarget {
                public static function method(string $value): string {
                    return 'owner:' . $value;
                }
            }
            ",
    )
    .expect("autoload target should be written");
    let source = "<?php
            spl_autoload_register(function ($class) {
                require __DIR__ . '/OwnerTarget.php';
            });
            echo StaticAutoloadOwnerTarget::method('ok');
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "owner:ok");
}

#[test]
fn included_instance_method_from_global_function_uses_declaring_unit() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-instance-include-owner-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("OwnerTarget.php"),
        "<?php
            function include_padding_one() {}
            function include_padding_two() {}
            class InstanceIncludeOwnerTarget {
                public function method(string $value): string {
                    return 'owner:' . $value;
                }
            }
            ",
    )
    .expect("include target should be written");
    let source = "<?php
            require __DIR__ . '/OwnerTarget.php';
            function entry_padding_one() {}
            function entry_padding_two() {}
            function display_setup_form($object) {
                return $object->method('ok');
            }
            echo display_setup_form(new InstanceIncludeOwnerTarget());
            echo '|', display_setup_form(new InstanceIncludeOwnerTarget());
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            execution_format: ExecutionFormat::Auto,
            quickening: QuickeningMode::On,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "owner:ok|owner:ok");
}

#[test]
fn conditional_included_instance_method_uses_declaring_unit() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-conditional-instance-owner-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("OwnerTarget.php"),
        "<?php
            function conditional_include_padding_one() {}
            function conditional_include_padding_two() {}
            if (!class_exists('ConditionalIncludeOwnerTarget', false)) {
                class ConditionalIncludeOwnerTarget {
                    public function method(string $value): string {
                        return 'owner:' . $value;
                    }
                }
            }
            ",
    )
    .expect("include target should be written");
    let source = "<?php
            require __DIR__ . '/OwnerTarget.php';
            function entry_padding_one() {}
            function entry_padding_two() {}
            function display_setup_form($object) {
                return $object->method('ok');
            }
            echo display_setup_form(new ConditionalIncludeOwnerTarget());
            echo '|', display_setup_form(new ConditionalIncludeOwnerTarget());
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            execution_format: ExecutionFormat::Auto,
            quickening: QuickeningMode::On,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "owner:ok|owner:ok");
}

#[test]
fn inherited_static_property_visibility_crosses_dynamic_include_units() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-static-property-dynamic-parent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("functions.php"),
        "<?php function translate($value) { return $value; }",
    )
    .expect("function include should be written");
    std::fs::write(
        root.join("Parent.php"),
        "<?php
            namespace Vendor\\Mail;
            class ParentMailer {
                protected static $language = array();
                public static function value() { return self::$language['ok']; }
            }
            ",
    )
    .expect("parent include should be written");
    std::fs::write(
        root.join("Child.php"),
        "<?php
            class ChildMailer extends Vendor\\Mail\\ParentMailer {
                public function __construct() { static::setLanguage(); }
                public static function setLanguage() {
                    static::$language = array('ok' => translate('done'));
                }
            }
            ",
    )
    .expect("child include should be written");
    let source = "<?php
            require __DIR__ . '/functions.php';
            require __DIR__ . '/Parent.php';
            require __DIR__ . '/Child.php';
            new ChildMailer();
            echo Vendor\\Mail\\ParentMailer::value();
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "done");
}

#[test]
fn function_local_class_is_visible_only_after_declaration_executes() {
    let result = execute_source(
        "<?php
            function declare_later() { class LaterDeclared {} }
            echo class_exists('LaterDeclared', false) ? 'early' : 'missing';
            declare_later();
            echo '|', class_exists('LaterDeclared', false) ? 'declared' : 'missing';
            echo '|', (new LaterDeclared())::class;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "missing|declared|LaterDeclared"
    );
}

#[test]
fn spl_autoload_unregister_spl_autoload_call_warns_and_clears_registry() {
    let result = execute_source(
        "<?php
            function first_loader($name) {}
            function second_loader($name) {}
            spl_autoload_register('first_loader');
            spl_autoload_register('second_loader');
            var_dump(spl_autoload_unregister('spl_autoload_call'));
            echo count(spl_autoload_functions()), '|';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains(
            "Deprecated: spl_autoload_unregister(): Using spl_autoload_call() as a callback for spl_autoload_unregister() is deprecated"
        ));
    assert!(output.contains("bool(true)\n0|"));
}

#[test]
fn spl_autoload_loads_lowercase_include_once_candidates() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-spl-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("testclass.inc"), "<?php echo 'inc|';\n")
        .expect("testclass.inc should be written");
    std::fs::write(root.join("testclass"), "<?php echo 'bare|';\n")
        .expect("testclass should be written");
    std::fs::write(
        root.join("testclass.class.inc"),
        "<?php echo 'class|'; class TestClass {}\n",
    )
    .expect("testclass.class.inc should be written");
    std::fs::write(
        root.join("registeredclass.class.inc"),
        "<?php echo 'registered|'; class RegisteredClass {}\n",
    )
    .expect("registeredclass.class.inc should be written");
    let source = "<?php
            spl_autoload('TestClass');
            spl_autoload('TestClass', null);
            spl_autoload('TestClass', '.inc,,.class.inc');
            echo class_exists('TestClass', false) ? 'loaded|' : 'missing|';
            echo spl_autoload_extensions('.class.inc'), '|';
            spl_autoload_register();
            echo class_exists('RegisteredClass') ? 'registered-loaded' : 'registered-missing';
        ";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "inc|bare|class|loaded|.class.inc|registered|registered-loaded"
    );
}

#[test]
fn spl_autoload_register_do_throw_false_notices_and_rejects_spl_autoload_call() {
    let result = execute_source(
        "<?php
            function loader_for_notice($name) {}
            spl_autoload_register('loader_for_notice', false);
            try {
                spl_autoload_register('spl_autoload_call');
            } catch (ValueError $e) {
                echo $e->getMessage(), '|';
            }
            echo count(spl_autoload_functions()), '|';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains(
            "Notice: spl_autoload_register(): Argument #2 ($do_throw) has been ignored, spl_autoload_register() will always throw"
        ));
    assert!(output.contains(
            "spl_autoload_register(): Argument #1 ($callback) must not be the spl_autoload_call() function|1|"
        ));
}

#[test]
fn spl_autoload_register_canonicalizes_object_static_method_callbacks() {
    let result = execute_source(
        "<?php
            class StaticLoader {
                static function load($name) { echo __METHOD__, ':', $name, '|'; }
            }
            spl_autoload_register(['StaticLoader', 'load']);
            spl_autoload_register([new StaticLoader(), 'load']);
            $callbacks = spl_autoload_functions();
            var_dump(count($callbacks), $callbacks[0]);
            spl_autoload_call('MissingStaticClass');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("int(1)\narray(2) {"));
    assert!(output.contains("string(12) \"StaticLoader\""));
    assert!(output.contains("string(4) \"load\""));
    assert!(output.contains("StaticLoader::load:MissingStaticClass|"));
}

#[test]
fn spl_autoload_functions_exposes_invokable_objects_as_objects() {
    let result = execute_source(
        "<?php
            class InvokableLoader {
                private $dir;
                function __construct($dir) { $this->dir = $dir; }
                function __invoke($name) {}
            }
            $loader = new InvokableLoader('src');
            spl_autoload_register($loader);
            $callbacks = spl_autoload_functions();
            var_dump($callbacks);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("array(1) {"));
    assert!(output.contains("object(InvokableLoader)#"));
    assert!(output.contains("[\"dir\":\"InvokableLoader\":private]=>"));
    assert!(!output.contains("string(8) \"__invoke\""));
}

#[test]
fn spl_autoload_register_invalid_methods_throw_php_type_errors() {
    let result = execute_source(
        "<?php
            class MyAutoLoader {
                static protected function noAccess($className) {}
                static function autoLoad($className) {}
                function dynaLoad($className) {}
            }
            $obj = new MyAutoLoader;
            foreach ([
                'MyAutoLoader::notExist',
                'MyAutoLoader::noAccess',
                'MyAutoLoader::autoLoad',
                'MyAutoLoader::dynaLoad',
                ['MyAutoLoader', 'notExist'],
                ['MyAutoLoader', 'noAccess'],
                ['MyAutoLoader', 'autoLoad'],
                ['MyAutoLoader', 'dynaLoad'],
                [$obj, 'notExist'],
                [$obj, 'noAccess'],
                [$obj, 'autoLoad'],
                [$obj, 'dynaLoad'],
            ] as $idx => $callback) {
                try {
                    spl_autoload_register($callback);
                    echo $idx, ':ok|';
                } catch (TypeError $e) {
                    echo $idx, ':', $e->getMessage(), '|';
                }
            }
            echo 'count=', count(spl_autoload_functions());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains(
            "0:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, class MyAutoLoader does not have a method \"notExist\"|"
        ));
    assert!(output.contains(
            "1:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method MyAutoLoader::noAccess()|"
        ));
    assert!(output.contains("2:ok|"));
    assert!(output.contains(
            "3:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, non-static method MyAutoLoader::dynaLoad() cannot be called statically|"
        ));
    assert!(output.contains(
            "4:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, class MyAutoLoader does not have a method \"notExist\"|"
        ));
    assert!(output.contains(
            "5:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method MyAutoLoader::noAccess()|"
        ));
    assert!(output.contains("6:ok|"));
    assert!(output.contains(
            "7:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, non-static method MyAutoLoader::dynaLoad() cannot be called statically|"
        ));
    assert!(output.contains(
            "8:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, class MyAutoLoader does not have a method \"notExist\"|"
        ));
    assert!(output.contains(
            "9:spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, cannot access protected method MyAutoLoader::noAccess()|"
        ));
    assert!(output.contains("10:ok|"));
    assert!(output.contains("11:ok|"));
    assert!(output.contains("count=2"));
}

#[test]
fn spl_autoload_register_preserves_requested_static_class_target() {
    let result = execute_source(
        "<?php
            class Test {
                public static function register() {
                    spl_autoload_register([Test::class, 'autoload']);
                }
                public static function autoload($class) {
                    echo 'self=', self::class, ', static=', static::class, '|';
                }
            }
            class Test2 extends Test {
                public static function register() {
                    spl_autoload_register([Test2::class, 'autoload']);
                }
            }
            Test::register();
            Test2::register();
            spl_autoload_call('MissingAutoloadScopeClass');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "self=Test, static=Test|self=Test, static=Test2|"
    );
}

#[test]
fn imported_class_constant_preserves_display_name_for_autoload() {
    let result = execute_source(
        r#"<?php
            namespace WpOrg\Requests;
            spl_autoload_register(function ($class) {
                echo "autoload:$class|";
                if ($class === 'WpOrg\Requests\Capability') {
                    interface Capability {
                        public const SSL = 'ssl';
                    }
                }
            });
            final class Requests {
                public static function request() {
                    return [Capability::SSL => true];
                }
            }
            $capabilities = Requests::request();
            echo Capability::SSL, '|';
            foreach ($capabilities as $key => $value) {
                echo $key, '=', $value ? 'true' : 'false';
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "autoload:WpOrg\\Requests\\Capability|ssl|ssl=true"
    );
}

#[test]
fn namespaced_new_object_preserves_display_name_for_autoload() {
    let result = execute_source(
        r#"<?php
            namespace SimplePie;
            spl_autoload_register(function ($class) {
                echo "autoload:$class|";
                if ($class === 'SimplePie\Exception') {
                    class Exception extends \Exception {}
                }
            });
            new Exception('x');
            echo 'ok';
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "autoload:SimplePie\\Exception|ok"
    );
}

#[test]
fn aliased_parent_declaration_preserves_display_name_for_autoload() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-parent-alias-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Exception.php"),
        "<?php
            namespace SimplePie;
            class Exception extends \\Exception {}
            ",
    )
    .expect("parent class target should be written");
    let source = r#"<?php
            namespace SimplePie\HTTP;
            use SimplePie\Exception as SimplePieException;
            spl_autoload_register(function ($class) {
                echo "autoload:$class|";
                if ($class === 'SimplePie\Exception') {
                    require __DIR__ . '/Exception.php';
                }
            });
            final class ClientException extends SimplePieException {}
            echo 'ok';
        "#;
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("autoload:SimplePie\\Exception|"),
        "{output}"
    );
    assert!(output.contains("ok"), "{output}");
    assert!(
        !output.contains("autoload:simplepie\\exception|"),
        "{output}"
    );
}

#[test]
fn qualified_parent_declaration_preserves_display_name_for_autoload() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-parent-qualified-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::create_dir_all(root.join("Requests")).expect("requests dir should be created");
    std::fs::write(
        root.join("Requests/Hooks.php"),
        "<?php
            namespace Vendor\\Requests;
            class Hooks {}
            ",
    )
    .expect("parent class target should be written");
    let source = r#"<?php
            spl_autoload_register(function ($class) {
                echo "autoload:$class|";
                if ($class === 'Vendor\Requests\Hooks') {
                    require __DIR__ . '/Requests/Hooks.php';
                }
            });
            class HttpRequestsHooks extends Vendor\Requests\Hooks {}
            echo 'ok';
        "#;
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("autoload:Vendor\\Requests\\Hooks|"),
        "{output}"
    );
    assert!(output.contains("ok"), "{output}");
    assert!(
        !output.contains("autoload:vendor\\requests\\hooks|"),
        "{output}"
    );
}

#[test]
fn namespaced_parent_declaration_preserves_display_name_for_autoload() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-parent-namespaced-autoload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let discovery_dir = root.join("Http/Discovery");
    std::fs::create_dir_all(&discovery_dir).expect("discovery dir should be created");
    std::fs::write(
        discovery_dir.join("ClassDiscovery.php"),
        "<?php
            namespace Vendor\\Package\\Http\\Discovery;
            class ClassDiscovery {}
            ",
    )
    .expect("parent class target should be written");
    let source = r#"<?php
            namespace Vendor\Package\Http\Discovery;
            spl_autoload_register(function ($class) {
                echo "autoload:$class|";
                if ($class === 'Vendor\Package\Http\Discovery\ClassDiscovery') {
                    require __DIR__ . '/Http/Discovery/ClassDiscovery.php';
                }
            });
            final class Psr18ClientDiscovery extends ClassDiscovery {}
            echo 'ok';
        "#;
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("autoload:Vendor\\Package\\Http\\Discovery\\ClassDiscovery|"),
        "{output}"
    );
    assert!(output.contains("ok"), "{output}");
    assert!(
        !output.contains("autoload:vendor\\package\\http\\discovery\\classdiscovery|"),
        "{output}"
    );
}

#[test]
fn spl_autoload_extensions_and_class_parents_are_request_local_builtins() {
    let result = execute_source(
        r#"<?php
            class BaseParent {}
            class ChildParent extends BaseParent {}
            echo spl_autoload_extensions(), "|";
            echo spl_autoload_extensions(".php"), "|", spl_autoload_extensions(), "|";
            foreach (class_parents(ChildParent::class) as $key => $value) {
                echo $key, "=", $value, "|";
            }
            var_dump(class_parents("MissingParent", false));
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with(".inc,.php|.php|.php|BaseParent=BaseParent|\n"));
    assert!(output.contains(
            "Warning: class_parents(): Class MissingParent does not exist in /tmp/phrust-test.php on line "
        ));
    assert!(output.ends_with("\nbool(false)\n"));
}

#[test]
fn class_implements_reports_userland_and_internal_interfaces() {
    let result = execute_source(
        r#"<?php
            interface BaseIface {}
            interface ChildIface extends BaseIface {}
            class ImplParent implements ChildIface {}
            class ImplChild extends ImplParent {}
            foreach (class_implements(ImplChild::class) as $key => $value) {
                echo $key, "=", $value, "|";
            }
            echo class_implements("MissingImpl", false) === false ? "missing|" : "bad|";
            foreach (class_implements(ArrayIterator::class) as $key => $value) {
                echo $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with("ChildIface=ChildIface|BaseIface=BaseIface|\n"));
    assert!(output.contains(
            "Warning: class_implements(): Class MissingImpl does not exist in /tmp/phrust-test.php on line "
        ));
    assert!(output.ends_with(
            "\nmissing|Iterator=Iterator|Traversable=Traversable|Countable=Countable|ArrayAccess=ArrayAccess|SeekableIterator=SeekableIterator|"
        ));
}

#[test]
fn ini_config_builtins_use_request_local_registry() {
    let result = execute_source(
        "<?php
            echo ini_get('default_charset'), \"\\n\";
            echo ini_get('missing.option') === false ? \"missing\\n\" : \"bad\\n\";
            echo ini_set('memory_limit', '64M'), \"\\n\";
            echo ini_get('memory_limit'), \"\\n\";
            echo get_cfg_var('memory_limit'), \"\\n\";
            echo (string) 1.75, \"\\n\";
            echo ini_set('precision', '0'), \"\\n\";
            echo (string) 1.75, \"\\n\";
            $flat = ini_get_all(null, false);
            echo $flat['memory_limit'], \"\\n\";
            $details = ini_get_all();
            echo $details['memory_limit']['global_value'], '|',
                 $details['memory_limit']['local_value'], '|',
                 $details['memory_limit']['access'], \"\\n\";
            $session = ini_get_all('session', false);
            echo $session['session.cookie_path'], '|',
                 isset($session['memory_limit']) ? 'bad' : 'filtered', \"\\n\";
            echo ignore_user_abort(), \"\\n\";
            echo ignore_user_abort(true), \"\\n\";
            echo ignore_user_abort(), \"\\n\";
            echo ini_get('ignore_user_abort'), \"\\n\";
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "UTF-8\nmissing\n128M\n64M\n128M\n1.75\n14\n2\n64M\n128M|64M|7\n/|filtered\n0\n0\n1\n1\n"
    );
}

#[test]
fn session_set_cookie_params_updates_request_ini() {
    let result = execute_source(
        "<?php
            ob_start();
            var_dump(ini_get('session.cookie_path'));
            var_dump(session_set_cookie_params(3600, '/foo'));
            var_dump(ini_get('session.cookie_lifetime'));
            var_dump(ini_get('session.cookie_path'));
            var_dump(session_start());
            var_dump(session_set_cookie_params(1800, '/bar'));
            var_dump(ini_get('session.cookie_lifetime'));
            var_dump(ini_get('session.cookie_path'));
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("string(1) \"/\""), "{output}");
    assert!(output.contains("bool(true)"), "{output}");
    assert!(output.contains("string(4) \"/foo\""), "{output}");
    assert!(output.contains("bool(false)"), "{output}");
    assert!(output.contains("string(4) \"3600\""), "{output}");
    assert!(!output.contains("string(4) \"/bar\""), "{output}");
    assert_eq!(
        output
            .matches("Session cookie parameters cannot be changed")
            .count(),
        1,
        "{output}"
    );
}

#[test]
fn session_auto_start_uses_startup_ini_before_user_code() {
    let result = execute_source_with_options(
        "<?php
            ob_start();
            var_dump(ini_get('session.auto_start'));
            var_dump(session_status());
            var_dump($_SESSION);
            var_dump(session_commit());
            var_dump(session_status());
            ob_end_flush();
            ",
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_ini_overrides(vec![("session.auto_start".to_owned(), "1".to_owned())]),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "string(1) \"1\"\nint(2)\narray(0) {\n}\nbool(true)\nint(1)\n"
    );
}

#[test]
fn session_start_discards_pre_start_session_global() {
    let result = execute_source(
        "<?php
            ob_start();
            $_SESSION['blah'] = 'foo';
            var_dump($_SESSION);
            var_dump(session_start());
            var_dump($_SESSION);
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array(1) {\n  [\"blah\"]=>\n  string(3) \"foo\"\n}\nbool(true)\narray(0) {\n}\n"
    );
}

#[test]
fn session_destroy_preserves_live_global_until_next_start() {
    let result = execute_source(
        "<?php
            ob_start();
            session_start();
            $_SESSION['blah'] = 'foo';
            var_dump(session_destroy());
            var_dump($_SESSION);
            var_dump(session_start());
            var_dump($_SESSION);
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\narray(1) {\n  [\"blah\"]=>\n  string(3) \"foo\"\n}\nbool(true)\narray(0) {\n}\n"
    );
}

#[test]
fn session_start_notice_mentions_auto_start_without_source_location() {
    let result = execute_source_with_options(
        "<?php
            ob_start();
            var_dump(session_start());
            ob_end_flush();
            ",
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_ini_overrides(vec![("session.auto_start".to_owned(), "1".to_owned())]),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains(
            "session_start(): Ignoring session_start() because a session is already active (session started automatically)"
        ),
        "{output}"
    );
    assert!(!output.contains("started from"), "{output}");
}

#[test]
fn session_encode_decode_supports_selected_serializers() {
    let result = execute_source(
        "<?php
            ob_start();
            var_dump(session_start());
            $_SESSION['foo'] = 123;
            $_SESSION['bar'] = 'baz';
            var_dump(session_encode());
            var_dump(session_decode('qux|a:1:{i:0;i:7;}'));
            var_dump($_SESSION['qux'][0]);
            session_write_close();

            ini_set('session.serialize_handler', 'php_serialize');
            var_dump(session_start());
            $_SESSION[-3] = 'foo';
            $_SESSION[3] = 'bar';
            $_SESSION['var'] = 123;
            var_dump(session_encode());
            session_write_close();

            ini_set('session.serialize_handler', 'php_binary');
            var_dump(session_start());
            var_dump(session_decode(\"\\x03binb:1;\"));
            var_dump($_SESSION['bin']);
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("string(24) \"foo|i:123;bar|s:3:\"baz\";\""),
        "{output}"
    );
    assert!(output.contains("bool(true)\nint(7)"), "{output}");
    assert!(
        output.contains("s:3:\"var\";i:123;"),
        "php_serialize handler should encode string keys: {output}"
    );
    assert!(
        output.contains("bool(true)\nbool(true)"),
        "php_binary decode should restore boolean value: {output}"
    );
}

#[test]
fn ini_set_rejects_unknown_session_serializer_handler() {
    let result = execute_source(
        "<?php
            ob_start();
            var_dump(ini_get('session.serialize_handler'));
            var_dump(ini_set('session.serialize_handler', 'wrong_handler'));
            var_dump(ini_get('session.serialize_handler'));
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("string(3) \"php\""),
        "default serializer should be visible before ini_set: {output}"
    );
    assert!(
        output.contains("ini_set(): Serialization handler \"wrong_handler\" cannot be found"),
        "{output}"
    );
    assert!(
        output.contains("bool(false)\nstring(3) \"php\""),
        "failed ini_set should return false and preserve the previous serializer: {output}"
    );
}

#[test]
fn session_decode_php_serializer_supports_top_level_reference_records() {
    let result = execute_source(
        "<?php
            ob_start();
            session_start();
            var_dump(session_decode('foo|a:1:{i:0;i:7;}guff|R:1;blah|R:1;'));
            var_dump($_SESSION);
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\narray(3) {\n  [\"foo\"]=>\n  &array(1) {\n    [0]=>\n    int(7)\n  }\n  [\"guff\"]=>\n  &array(1) {\n    [0]=>\n    int(7)\n  }\n  [\"blah\"]=>\n  &array(1) {\n    [0]=>\n    int(7)\n  }\n}\n"
    );
}

#[test]
fn session_encode_php_serializer_supports_shared_top_level_references() {
    let result = execute_source(
        "<?php
            ob_start();
            session_start();
            $array = array(1, 2, 3);
            $_SESSION['foo'] = &$array;
            $_SESSION['guff'] = &$array;
            $_SESSION['blah'] = &$array;
            var_dump(session_encode());
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "string(52) \"foo|a:3:{i:0;i:1;i:1;i:2;i:2;i:3;}guff|R:1;blah|R:1;\"\n"
    );
}

#[test]
fn session_encode_php_serializer_supports_recursive_references() {
    let result = execute_source(
        "<?php
            ob_start();
            session_start();
            $array = array(1, 2, 3);
            $array['foo'] = &$array;
            $array['blah'] = &$array;
            $_SESSION['data'] = &$array;
            var_dump(session_encode());
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "string(64) \"data|a:5:{i:0;i:1;i:1;i:2;i:2;i:3;s:3:\"foo\";R:1;s:4:\"blah\";R:1;}\"\n"
    );
}

#[test]
fn session_module_name_rejects_unknown_user_and_active_changes() {
    let result = execute_source(
        "<?php
            ob_start();
            var_dump(session_module_name('blah'));
            var_dump(session_module_name());
            session_start();
            var_dump(session_module_name('files'));
            session_destroy();
            try {
                session_module_name('user');
            } catch (ValueError $e) {
                echo $e->getMessage(), \"\\n\";
            }
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("session_module_name(): Session handler module \"blah\" cannot be found"),
        "{output}"
    );
    assert!(
        output.contains("bool(false)\nstring(5) \"files\""),
        "{output}"
    );
    assert!(
        output.contains("Session save handler module cannot be changed when a session is active"),
        "{output}"
    );
    assert!(
        output.contains("session_module_name(): Argument #1 ($module) cannot be \"user\""),
        "{output}"
    );
}

#[test]
fn session_start_files_handler_rejects_missing_save_path() {
    let result = execute_source(
        "<?php
            ob_start();
            ini_set('session.save_handler', 'files');
            ini_set('session.save_path', '/phrust-missing-session-dir');
            var_dump(session_start());
            var_dump(session_status());
            var_dump(session_destroy());
            ob_end_flush();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("session_start(): open(/phrust-missing-session-dir/"),
        "{output}"
    );
    assert!(
        output.contains("session_start(): Failed to read session data: files (path: /phrust-missing-session-dir)"),
        "{output}"
    );
    assert!(
        output.contains("bool(false)\nint(1)"),
        "failed start should leave session inactive: {output}"
    );
    assert!(
        output.contains("session_destroy(): Trying to destroy uninitialized session"),
        "{output}"
    );
}

#[test]
fn ini_set_session_save_path_respects_open_basedir() {
    let root = std::env::temp_dir().join(format!(
        "phrust-session-open-basedir-{}",
        std::process::id()
    ));
    let allowed = root.join("allowed");
    let outside = root.join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed directory should be created");
    std::fs::create_dir_all(&outside).expect("outside directory should be created");
    let outside_literal = outside
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    let source = format!(
        "<?php
            ob_start();
            ini_set('session.save_handler', 'files');
            ini_set('open_basedir', '.');
            var_dump(ini_set('session.save_path', '{outside_literal}'));
            var_dump(session_save_path());
            var_dump(session_start());
            ob_end_flush();
            "
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::default().with_cwd(allowed),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("ini_set(): open_basedir restriction in effect."),
        "{output}"
    );
    assert!(output.contains("bool(false)\nstring(0) \"\""), "{output}");
    assert!(
        output.contains("session_start(): open_basedir restriction in effect."),
        "{output}"
    );
    assert!(
        output.contains("session_start(): Failed to initialize storage module: files (path: )"),
        "{output}"
    );
}

#[test]
fn date_timezone_builtins_use_request_local_registry() {
    let result = execute_source(
        "<?php
            echo date_default_timezone_get(), \"\\n\";
            date_default_timezone_set('Europe/Berlin');
            echo date_default_timezone_get(), \"\\n\";
            echo date('Y-m-d H:i:s T', 0), \"\\n\";
            echo gmdate('Y-m-d H:i:s T', 0), \"\\n\";
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "UTC\nEurope/Berlin\n1970-01-01 01:00:00 CET\n1970-01-01 00:00:00 GMT\n"
    );

    let separate = execute_source("<?php echo date_default_timezone_get();");
    assert!(separate.status.is_success(), "{:?}", separate.status);
    assert_eq!(separate.output.as_bytes(), b"UTC");
}

#[test]
fn date_time_runtime_classes_dispatch_methods() {
    let result = execute_source(
            "<?php
            $zone = new DateTimeZone('UTC');
            echo $zone->getName(), \"\\n\";
            $date = new DateTime('2024-01-02 03:04:05', $zone);
            echo $date->format('Y-m-d H:i:s T U'), \"\\n\";
            echo $date->getTimestamp(), \"\\n\";
            echo $date->getTimezone()->getName(), \"\\n\";
            echo $date->setTimezone(new DateTimeZone('+01:00'))->format('H:i P'), \"\\n\";
            echo call_user_func([$date, 'setTimezone'], new DateTimeZone('+02:00'))->format('H:i P'), \"\\n\";
            echo $date->getOffset(), '|', $zone->getOffset($date), \"\\n\";
            $date->setTimezone($zone);
            echo DateTime::ATOM, '|', DateTimeImmutable::RFC3339_EXTENDED, '|', DateTimeInterface::RFC7231, \"\\n\";
            echo DateTimeZone::ALL_WITH_BC, \"\\n\";
            echo (new datetimezone('UTC'))->getName(), \"\\n\";
            $offsetZone = new DateTimeZone('+00:00');
            echo $offsetZone->getName(), \"\\n\";
            $offsetDate = new DateTime('1970-01-01 00:00:00', new DateTimeZone('-0530'));
            echo $offsetDate->format('U P O T'), \"\\n\";
            date_default_timezone_set('Europe/Berlin');
            $local = new DateTime('2024-01-02 03:04:05');
            echo $local->format('Y-m-d H:i:s T U'), \"\\n\";
            $interval = new DateInterval('P1DT2H');
            echo $interval->d, '|', $interval->h, '|', $interval->format('%d %h %i %s'), \"\\n\";
            echo $date->add($interval)->format('Y-m-d H:i:s'), \"\\n\";
            $immutable = new DateTimeImmutable('2024-01-02 00:00:00', $zone);
            $changed = $immutable->add(new DateInterval('P1D'));
            echo $immutable->format('Y-m-d'), '|', $changed->format('Y-m-d'), \"\\n\";
            echo strtotime('next day', 0), '|', strtotime('-1 day', 86400), \"\\n\";
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "UTC\n2024-01-02 03:04:05 UTC 1704164645\n1704164645\nUTC\n04:04 +01:00\n05:04 +02:00\n7200|0\nY-m-d\\TH:i:sP|Y-m-d\\TH:i:s.vP|D, d M Y H:i:s \\G\\M\\T\n4095\nUTC\n+00:00\n19800 -05:30 -0530 GMT-0530\n2024-01-02 03:04:05 CET 1704161045\n1|2|1 2 0 0\n2024-01-03 05:04:05\n2024-01-02|2024-01-03\n86400|0\n"
    );
}

#[test]
fn mysqli_runtime_objects_expose_client_properties() {
    let result = execute_source(
        "<?php
            $mysqli = new mysqli();
            echo $mysqli->client_info, '|', $mysqli->client_version, \"\\n\";
            $init = mysqli_init();
            echo $init->client_info, '|', $init->client_version, \"\\n\";
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "mysqlnd 8.5.7|80507\nmysqlnd 8.5.7|80507\n"
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn error_handling_builtins_use_request_local_handler_stack() {
    let result = execute_source(
            "<?php
            function first($errno, $errstr, $errfile, $errline) { echo 'first:', $errno, ':', $errstr, \"\\n\"; return true; }
            function second($errno, $errstr, $errfile, $errline) { echo 'second:', $errno, ':', $errstr, \"\\n\"; return true; }
            echo set_error_handler('first') === null ? \"first-null\\n\" : \"bad\\n\";
            set_error_handler('second');
            echo \"second-set\\n\";
            trigger_error('top', E_USER_WARNING);
            restore_error_handler();
            user_error('restored', E_USER_WARNING);
            restore_error_handler();
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "first-null\nsecond-set\nsecond:512:top\nfirst:512:restored\n"
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn error_log_builtin_supports_default_and_file_append_modes() {
    let path = std::env::temp_dir().join(format!(
        "phrust-vm-error-log-{}-{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let escaped_path = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    let source = format!(
        "<?php
            var_dump(error_log('default message'));
            var_dump(error_log('file message', 3, '{escaped_path}'));
            "
    );
    let result = execute_source(&source);
    let file_contents = std::fs::read_to_string(&path).expect("log file should be written");
    let _ = std::fs::remove_file(&path);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "bool(true)\nbool(true)\n");
    assert_eq!(file_contents, "file message");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn trigger_error_respects_handler_return_reporting_and_display() {
    let result = execute_source(
            "<?php
            function unhandled($errno, $errstr, $errfile, $errline) { echo 'handler:', $errstr, \"\\n\"; return false; }
            set_error_handler('unhandled', E_USER_WARNING);
            ini_set('display_errors', '0');
            trigger_error('hidden', E_USER_WARNING);
            error_reporting(0);
            trigger_error('masked', E_USER_WARNING);
            echo 'done', \"\\n\";
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "handler:hidden\nhandler:masked\ndone\n"
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_USER_WARNING");
}

#[test]
fn internal_builtin_diagnostics_respect_error_handler() {
    let result = execute_source(
        "<?php
            function capture_warning($errno, $errstr) { echo $errno, ':', $errstr, \"\\n\"; }
            set_error_handler('capture_warning');
            $result = iconv_mime_decode('Subject: =?ISO-8859-1?Q?Pr=FCfung??', 0, 'UTF-8');
            echo strlen($result);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "2:iconv_mime_decode(): Malformed string\n0"
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn error_reporting_getter_setter_masks_are_request_local_bitmasks() {
    let result = execute_source(
        "<?php
            $old = error_reporting();
            echo is_int($old) ? \"old-int\\n\" : \"bad\\n\";
            error_reporting(E_ALL & ~E_DEPRECATED);
            echo error_reporting() === (E_ALL & ~E_DEPRECATED) ? \"mask-ok\\n\" : \"mask-bad\\n\";
            error_reporting($old);
            echo error_reporting() === $old ? \"restore-ok\\n\" : \"restore-bad\\n\";
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "old-int\nmask-ok\nrestore-ok\n"
    );
}

#[test]
fn error_suppression_suppresses_one_warning_and_restores_reporting() {
    let result = execute_source("<?php echo @$missing, 'suppressed|'; echo $missing, 'restored';");

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with("suppressed|\nWarning: Undefined variable $missing in "),
        "{output}"
    );
    assert!(output.ends_with("restored"), "{output}");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING"
    );
}

#[test]
fn bootstrap_warning_sources_respect_display_and_reporting_masks() {
    let root =
        std::env::temp_dir().join(format!("phrust-vm-warning-sources-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp warning root should be created");
    let source = "<?php
            echo file_get_contents('missing.txt') === false ? 'read-false|' : 'bad|';
            echo file_put_contents('/missing-dir/out.txt', 'x') === false ? 'write-false|' : 'bad|';
            echo [1, 2];
            $s = 'abc';
            echo $s[9];
            trigger_error('notice', E_USER_NOTICE);
            trigger_error('warn', E_USER_WARNING);
            echo 'done';
            ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("read-false|"), "{output}");
    assert!(output.contains("write-false|"), "{output}");
    assert!(
        output.contains("Warning: Array to string conversion"),
        "{output}"
    );
    assert!(
        output.contains("Warning: Uninitialized string offset"),
        "{output}"
    );
    assert!(output.contains("Notice: notice in "), "{output}");
    assert!(output.contains("Warning: warn in "), "{output}");
    assert!(output.ends_with("done"), "{output}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_VM_USER_NOTICE"),
        "{:#?}",
        result.diagnostics
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_VM_USER_WARNING"),
        "{:#?}",
        result.diagnostics
    );
}

#[test]
fn exception_handler_stack_tracks_previous_handlers() {
    let result = execute_source(
        "<?php
            function ex1($e) {}
            function ex2($e) {}
            echo set_exception_handler('ex1') === null ? \"first\\n\" : \"bad\\n\";
            set_exception_handler('ex2');
            echo \"second\\n\";
            echo restore_exception_handler() ? \"restore1\\n\" : \"bad\\n\";
            echo restore_exception_handler() ? \"restore2\\n\" : \"bad\\n\";
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "first\nsecond\nrestore1\nrestore2\n"
    );
}

#[test]
fn uncaught_exceptions_call_registered_exception_handler() {
    let result = execute_source(
        "<?php
            function on_exception($e) { echo 'handled:', $e->getMessage(); }
            set_exception_handler('on_exception');
            throw new Exception('boom');
            echo 'after';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "handled:boom");
}

#[test]
fn fatal_user_errors_are_not_recovered_by_error_handler() {
    let result = execute_source(
        "<?php
            function swallow($errno, $errstr, $errfile, $errline) { echo 'handler'; return true; }
            set_error_handler('swallow');
            trigger_error('fatal', E_USER_ERROR);
            echo 'after';
            ",
    );

    assert!(!result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Fatal error: fatal in "));
    assert!(output.contains(" on line "));
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("E_PHP_VM_USER_ERROR"))
    );
}

#[test]
fn direct_builtin_type_and_value_errors_are_catchable_throwables() {
    let result = execute_source(
            "<?php
            try { strlen(); } catch (ArgumentCountError $e) { echo 'arity'; }
            echo '|';
            try { strlen(); } catch (TypeError $e) { echo 'arity-type'; }
            echo '|';
            try { strlen([]); } catch (TypeError $e) { echo 'type'; }
            echo '|';
            try { explode('', 'abc'); } catch (ValueError $e) { echo 'value'; }
            echo '|';
            try { json_decode('{', false, 512, JSON_THROW_ON_ERROR); } catch (JsonException $e) { echo 'json:', $e->getMessage(); }
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "arity|arity-type|type|value|json:Syntax error"
    );
}

#[test]
fn json_throw_on_error_exception_debug_shape_matches_php() {
    let result = execute_source(
        "<?php
            try {
                json_decode('{', false, 512, JSON_THROW_ON_ERROR);
            } catch (JsonException $e) {
                var_dump($e);
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    let normalized = normalize_object_debug_ids(&output);
    assert!(
        normalized.contains("object(JsonException)#%d (7) {"),
        "{output}"
    );
    assert!(
        output.contains("[\"message\":protected]=>\n  string(12) \"Syntax error\""),
        "{output}"
    );
    assert!(
        output.contains("[\"string\":\"Exception\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"trace\":\"Exception\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"previous\":\"Exception\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"function\"]=>\n      string(11) \"json_decode\""),
        "{output}"
    );
    assert!(
        output.contains("[\"args\"]=>\n      array(4) {"),
        "{output}"
    );
}

#[test]
fn generated_arginfo_drives_builtin_scalar_coercion_mode() {
    let weak = execute_source("<?php echo strlen(42), '|', strtoupper(42);");
    assert!(weak.status.is_success(), "{:?}", weak.status);
    assert_eq!(weak.output.as_bytes(), b"2|42");

    let strict = execute_source(
        "<?php declare(strict_types=1);
            try { strlen(42); } catch (TypeError $e) { echo 'strlen'; }
            echo '|';
            try { ord(49); } catch (TypeError $e) { echo 'ord'; }
            ",
    );
    assert!(strict.status.is_success(), "{:?}", strict.status);
    assert_eq!(strict.output.as_bytes(), b"strlen|ord");
}

#[test]
fn str_split_chunks_strings_and_reports_invalid_length() {
    let result = execute_source(
        "<?php
            echo json_encode(str_split('abc')), '|';
            echo json_encode(str_split('abcd', 2)), '|';
            echo json_encode(str_split('')), '|';
            echo json_encode(str_split(string: 'wxyz', length: 3)), '|';
            try { str_split('abc', 0); } catch (ValueError $e) { echo $e->getMessage(); }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "[\"a\",\"b\",\"c\"]|[\"ab\",\"cd\"]|[]|[\"wxy\",\"z\"]|str_split(): Argument #2 ($length) must be greater than 0"
    );
}

#[test]
fn json_encode_invokes_jsonserializable_userland_methods() {
    let result = execute_source(
        "<?php
            class Box implements JsonSerializable {
                public $value;
                public function __construct($value) { $this->value = $value; }
                public function jsonSerialize(): mixed { return ['value' => $this->value]; }
            }
            class SelfBox implements JsonSerializable {
                public $value = 3;
                public function jsonSerialize(): mixed { return $this; }
            }
            echo json_encode(new Box(2)), '|', json_encode(new SelfBox());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), br#"{"value":2}|{"value":3}"#);
}

#[test]
fn json_encode_jsonserializable_partial_recursion_sets_last_error() {
    let result = execute_source(
            "<?php
            class RecursiveBox implements JsonSerializable {
                public $value = 'x';
                public function jsonSerialize(): mixed {
                    return ['value' => $this->value, 'self' => $this];
                }
            }
            echo json_encode(new RecursiveBox(), JSON_PARTIAL_OUTPUT_ON_ERROR), '|', json_last_error();
            ",
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), br#"{"value":"x","self":null}|6"#);
}

#[test]
fn json_encode_jsonserializable_nested_self_encode_reports_recursion() {
    let result = execute_source(
        "<?php
            class RecursiveEncode implements JsonSerializable {
                public $a = 1;
                public function jsonSerialize(): mixed {
                    var_dump(json_encode($this));
                    return $this;
                }
            }
            var_dump(json_encode(new RecursiveEncode()));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"bool(false)\nstring(7) \"{\"a\":1}\"\n"
    );
}

#[test]
fn json_encode_jsonserializable_debug_info_recursion_matches_php() {
    let result = execute_source(
        "<?php
            class SerializingTest implements JsonSerializable {
                public $a = 1;
                public function __debugInfo() {
                    return [ 'result' => json_encode($this) ];
                }
                public function jsonSerialize(): mixed {
                    var_dump($this);
                    return $this;
                }
            }
            var_dump(json_encode(new SerializingTest()));
            echo \"---------\n\";
            var_dump(new SerializingTest());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        normalize_object_debug_ids(&result.output.to_string_lossy()),
        "object(SerializingTest)#%d (1) {\n  [\"result\"]=>\n  bool(false)\n}\nstring(7) \"{\"a\":1}\"\n---------\n*RECURSION*\nobject(SerializingTest)#%d (1) {\n  [\"result\"]=>\n  string(7) \"{\"a\":1}\"\n}\n"
    );
}

#[test]
fn print_r_uses_debug_info_for_jsonserializable_callbacks() {
    let result = execute_source(
        "<?php
            class SerializingTest implements JsonSerializable {
                public $a = 1;
                public function __debugInfo() {
                    return [ 'result' => $this->a ];
                }
                public function jsonSerialize(): mixed {
                    print_r($this);
                    return $this;
                }
            }
            var_dump(json_encode(new SerializingTest()));
            echo \"---------\n\";
            var_dump(new SerializingTest());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        normalize_object_debug_ids(&result.output.to_string_lossy()),
        "SerializingTest Object\n(\n    [result] => 1\n)\nstring(7) \"{\"a\":1}\"\n---------\nobject(SerializingTest)#%d (1) {\n  [\"result\"]=>\n  int(1)\n}\n"
    );
}

#[test]
fn hash_context_var_dump_uses_php_debug_info_shape() {
    let result = execute_source(
        "<?php
            var_dump(hash_init('sha256'));
            var_dump(hash_init('sha3-512'));
            $ctx = hash_init('sha256');
            var_dump(method_exists($ctx, '__debugInfo'));
            var_dump($ctx->__debugInfo());
            try {
                $ctx->__debugInfo(1);
            } catch (ArgumentCountError $e) {
                echo $e->getMessage(), \"\\n\";
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        normalize_object_debug_ids(&result.output.to_string_lossy()),
        "object(HashContext)#%d (1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\nobject(HashContext)#%d (1) {\n  [\"algo\"]=>\n  string(8) \"sha3-512\"\n}\nbool(true)\narray(1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\nHashContext::__debugInfo() expects exactly 0 arguments, 1 given\n"
    );
}

#[test]
fn generated_arginfo_deprecations_respect_error_reporting() {
    let result = execute_source(
        "<?php
            error_reporting(2047);
            $parts = explode(',', NULL);
            echo count($parts), ':', $parts[0], '|';
            try {
                explode('', NULL);
            } catch (ValueError $e) {
                echo $e->getMessage();
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "1:|explode(): Argument #1 ($separator) must not be empty"
    );
}

#[test]
fn invalid_array_operand_errors_are_catchable_type_errors() {
    let result = execute_source(
        "<?php
            try { [] - []; } catch (TypeError $e) { echo 'invalid:', $e->getMessage(); }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "invalid:Unsupported operand types: array - array"
    );
}

#[test]
fn output_buffering_builtins_support_nested_clean_and_flush() {
    let result = execute_source(
        "<?php
            echo 'root|';
            ob_start();
            echo 'a';
            ob_start();
            echo 'b';
            $level = ob_get_level();
            $inner_length = ob_get_length();
            $contents = ob_get_contents();
            $inner = ob_get_clean();
            echo 'c';
            $outer_length = ob_get_length();
            ob_end_flush();
            echo '|', $level, ':', $inner_length, ':', $contents, ':', $inner, ':', $outer_length;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "root|ac|2:1:b:b:2");
}

#[test]
fn output_buffering_ob_get_flush_returns_and_flushes_active_buffer() {
    let result = execute_source(
        "<?php
            echo 'root|';
            ob_start();
            echo 'a';
            ob_start();
            echo 'b';
            $flushed = ob_get_flush();
            echo 'c';
            $outer = ob_get_clean();
            var_dump(ob_get_flush());
            echo '|', $flushed, ':', $outer;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "root|bool(false)\n|b:abc");
}

#[test]
fn output_buffers_survive_caught_exceptions() {
    let result = execute_source(
        "<?php
            ob_start();
            try {
                echo 'before|';
                throw new Exception('boom');
            } catch (Exception $e) {
                echo 'catch|';
            }
            echo ob_get_clean();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "before|catch|");
}

#[test]
fn flush_pushes_active_buffers_to_root_without_closing_them() {
    let result = execute_source(
        "<?php
            echo 'root|';
            ob_start();
            echo 'outer';
            ob_start();
            echo 'inner';
            flush();
            echo 'tail';
            $level = ob_get_level();
            ob_end_flush();
            ob_end_flush();
            echo '|level=', $level;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "root|outerinnertail|level=2"
    );
}

#[test]
fn echo_writes_to_root_when_no_output_buffer_is_active() {
    let result = execute_source("<?php echo 'plain'; flush(); echo '|tail';");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "plain|tail");
}

#[test]
fn output_fast_paths_preserve_echo_print_buffering_and_report_counters() {
    let result = execute_source_with_options(
        "<?php
            echo 'a', 'b', true, false, null, 7, \"\\n\";
            echo print 'p';
            echo \"\\n\";
            ob_start();
            echo 'x', 'y';
            ob_end_flush();
            echo \"\\n\";
            ",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let expected = "ab17\np1\nxy\n";
    assert_eq!(result.output.to_string_lossy(), expected);
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.output_bytes, expected.len() as u64);
    assert!(counters.output_buffer_appends > 0, "{counters:?}");
    assert!(counters.output_fast_appends >= 3, "{counters:?}");
    assert!(counters.output_batched_appends >= 2, "{counters:?}");
    assert!(counters.output_batch_bytes >= 4, "{counters:?}");
    assert_eq!(counters.output_buffer_flushes, 1);
    assert!(
        counters.output_slow_appends_by_reason.is_empty(),
        "{counters:?}"
    );
}

#[test]
fn output_batching_preserves_nested_buffers_and_binary_scalar_bytes() {
    let result = execute_source_with_options(
        "<?php
            echo \"A\\x00\", 12, true, false, null, \"Z\";
            ob_start();
            echo 'inner', '-', 34, true;
            $captured = ob_get_clean();
            echo '|', $captured, '|tail';
            ",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"A\x00121Z|inner-341|tail");
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.output_batched_appends >= 2, "{counters:?}");
    assert!(counters.output_batch_bytes >= 12, "{counters:?}");
    assert_eq!(counters.output_buffer_flushes, 0, "{counters:?}");
    assert!(
        counters.output_slow_appends_by_reason.is_empty(),
        "{counters:?}"
    );
}

#[test]
fn semantic_helpers_echo_fast_hit_shared_by_rich_and_dense() {
    let source = "<?php echo 'a', 1, true, null, false;";
    let rich = execute_source_with_options(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Ir,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    let dense = execute_source_with_options(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(rich.status.is_success(), "{:?}", rich.status);
    assert!(dense.status.is_success(), "{:?}", dense.status);
    assert_eq!(rich.output.as_bytes(), b"a11");
    assert_eq!(dense.output.as_bytes(), rich.output.as_bytes());

    let rich_counters = rich.counters.expect("rich counters should be collected");
    assert!(rich_counters.output_fast_appends >= 1, "{rich_counters:?}");
    assert!(
        rich_counters.output_slow_appends_by_reason.is_empty(),
        "{rich_counters:?}"
    );

    let dense_counters = dense.counters.expect("dense counters should be collected");
    assert!(
        dense_counters.dense_functions_executed >= 1,
        "{dense_counters:?}"
    );
    assert!(
        dense_counters
            .dense_instruction_families_executed
            .get("output")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{dense_counters:?}"
    );
    assert!(
        dense_counters.output_fast_appends >= 1,
        "{dense_counters:?}"
    );
    assert!(
        dense_counters.output_slow_appends_by_reason.is_empty(),
        "{dense_counters:?}"
    );
}

#[test]
fn semantic_helpers_echo_fallback_reason_shared_by_rich_and_dense() {
    let source = "<?php echo [1], \"\\n\";";
    let rich = execute_source_with_options(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Ir,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    let dense = execute_source_with_options(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(rich.status.is_success(), "{:?}", rich.status);
    assert!(dense.status.is_success(), "{:?}", dense.status);
    let rich_output = rich.output.to_string_lossy();
    let dense_output = dense.output.to_string_lossy();
    assert_eq!(dense_output, rich_output);
    assert!(rich_output.contains("Array\n"), "{rich_output}");

    let rich_counters = rich.counters.expect("rich counters should be collected");
    assert_eq!(
        rich_counters
            .output_slow_appends_by_reason
            .get("array_conversion_warning"),
        Some(&1),
        "{rich_counters:?}"
    );

    let dense_counters = dense.counters.expect("dense counters should be collected");
    assert!(
        dense_counters.dense_functions_executed >= 1,
        "{dense_counters:?}"
    );
    assert!(
        dense_counters
            .dense_instruction_families_executed
            .get("output")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{dense_counters:?}"
    );
    assert_eq!(
        dense_counters
            .output_slow_appends_by_reason
            .get("array_conversion_warning"),
        Some(&1),
        "{dense_counters:?}"
    );
}

#[test]
fn output_fast_paths_preserve_to_string_fallback_and_conversion_errors() {
    let object = execute_source_with_options(
        "<?php
            class S {
                public function __toString(): string {
                    echo 'side|';
                    return 'object';
                }
            }
            echo new S(), '|done';
            ",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(object.status.is_success(), "{:?}", object.status);
    assert_eq!(object.output.to_string_lossy(), "side|object|done");
    let counters = object.counters.expect("counters should be collected");
    assert_eq!(
        counters
            .output_slow_appends_by_reason
            .get("object_to_string"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("output.object_to_string"),
        Some(&1),
        "{counters:?}"
    );
    assert!(counters.output_fast_appends >= 2, "{counters:?}");

    let root =
        std::env::temp_dir().join(format!("phrust-vm-output-resource-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp resource root should be created");
    let fallback = execute_source_with_options(
        "<?php
            echo 'before|';
            echo [1];
            $handle = fopen('data.txt', 'w+');
            echo $handle;
            ",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(fallback.status.is_success(), "{:?}", fallback.status);
    assert_eq!(
        fallback.output.to_string_lossy(),
        "before|\nWarning: Array to string conversion in <unknown> on line 0\nArrayResource id #1"
    );
    let counters = fallback.counters.expect("counters should be collected");
    assert_eq!(
        counters
            .output_slow_appends_by_reason
            .get("array_conversion_warning"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .output_slow_appends_by_reason
            .get("resource_conversion"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("output.array_conversion_warning"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("output.resource_conversion"),
        Some(&1),
        "{counters:?}"
    );

    let throwing = execute_source(
        "<?php
            class Bad {
                public function __toString(): string {
                    throw new Exception('boom');
                }
            }
            echo 'before|';
            echo new Bad();
            echo 'after';
            ",
    );

    assert!(!throwing.status.is_success(), "{:?}", throwing.status);
    assert_uncaught_exception_output_prefix(
        &throwing.output.to_string_lossy(),
        "before|",
        "Exception",
        "boom",
    );
}

#[test]
fn object_to_string_errors_are_catchable_in_cast_echo_and_printf() {
    let result = execute_source(
        "<?php
            class Plain {}
            class BadString {
                public function __toString(): string {
                    return [];
                }
            }
            class GoodString {
                public function __toString(): string {
                    echo 'good-call:', \"\\n\";
                    return 'good';
                }
            }
            try {
                var_dump((string) new Plain());
            } catch (Error $e) {
                echo 'cast:', $e->getMessage(), \"\\n\";
            }
            try {
                echo new BadString();
            } catch (Error $e) {
                echo 'echo:', $e->getMessage(), \"\\n\";
            }
            try {
                printf(new Plain());
            } catch (TypeError $e) {
                echo 'printf:', $e->getMessage(), \"\\n\";
            }
            try {
                printf(new BadString());
            } catch (TypeError $e) {
                echo 'printf-to-string:', $e->getMessage(), \"\\n\";
            }
            $array = [];
            try {
                echo $array[new Plain()];
            } catch (Error $e) {
                echo 'array-key:', $e->getMessage();
            }
            echo \"\\n\", sprintf('%s', new GoodString());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "cast:Object of class Plain could not be converted to string\n\
echo:BadString::__toString(): Return value must be of type string, array returned\n\
printf:printf(): Argument #1 ($format) must be of type string, Plain given\n\
printf-to-string:BadString::__toString(): Return value must be of type string, array returned\n\
array-key:Cannot access offset of type Plain on array\n\
good-call:\n\
good"
    );
}

#[test]
fn included_object_to_string_resolves_dynamic_class_method_for_sprintf() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-tostring-sprintf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("Theme.php"),
        "<?php
            class IncludedTheme {
                public function __toString(): string {
                    return 'Twenty Twenty-Five';
                }
            }
            ",
    )
    .expect("theme class should be written");
    let source = "<?php
            require __DIR__ . '/Theme.php';
            echo sprintf('<a href=\"themes.php\">%1$s</a>', new IncludedTheme());
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "<a href=\"themes.php\">Twenty Twenty-Five</a>"
    );
}

#[test]
fn internal_string_builtins_use_userland_to_string() {
    let result = execute_source(
        "<?php
            class S {
                public function __toString(): string {
                    return 'abc';
                }
            }
            echo strlen(new S()), '|', strtoupper(new S()), '|', strpos(new S(), 'b');
            echo '|', strtr('abc', new S(), 'XYZ');
            try {
                strtr('abc', new S());
            } catch (TypeError $e) {
                echo '|', $e->getMessage();
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "3|ABC|1|XYZ|strtr(): Argument #2 ($from) must be of type array, S given"
    );
}

#[test]
fn custom_validated_builtins_bypass_generated_arginfo_coercions() {
    let result = execute_source(
        "<?php
            class AllowTags {
                public function __toString(): string {
                    return 'ignored';
                }
            }
            echo strip_tags('<b>x</b>', new AllowTags()), '|';
            try {
                strrpos('t', 't', PHP_INT_MAX + 1);
            } catch (TypeError $e) {
                echo $e->getMessage(), '|';
            }
            try {
                hash_equals(123, 'NaN');
            } catch (TypeError $e) {
                echo $e->getMessage(), '|';
            }
            try {
                vprintf('%s', true);
            } catch (TypeError $e) {
                echo $e->getMessage(), '|';
            }
            try {
                filter_var_array([], '');
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "x|strrpos(): Argument #3 ($offset) must be of type int, float given|hash_equals(): Argument #1 ($known_string) must be of type string, int given|vprintf(): Argument #2 ($values) must be of type array, true given|filter_var_array(): Argument #2 ($options) must be of type array|int, string given"
    );
}

#[test]
fn builtin_deprecation_before_value_error_remains_catchable() {
    let result = execute_source(
        "<?php
            try {
                hash_init('xxh3', 0, '', ['secret' => 42]);
            } catch (ValueError $e) {
                echo '|', $e->getMessage();
            }
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains(
            "Deprecated: hash_init(): Passing a secret of a type other than string is deprecated"
        ),
        "{output}"
    );
    assert!(
        output.contains("|xxh3: Secret length must be >= 136 bytes, 2 bytes passed"),
        "{output}"
    );
}

#[test]
fn output_buffer_callback_gap_is_preserved_by_fast_paths() {
    let result = execute_source("<?php ob_start('strtolower'); echo 'unreachable';");

    assert!(!result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "");
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("E_PHP_VM_OUTPUT_BUFFER_CALLBACK_UNSUPPORTED")),
        "{:?}",
        result.status
    );
}

#[test]
fn empty_dim_uses_record_shape_fast_path_and_preserves_semantics() {
    let result = execute_source_with_options(
        "<?php\n\
             $a = ['x' => 1, 'y' => 0, 'z' => 'hi', 'w' => ''];\n\
             var_dump(empty($a['x']));\n\
             var_dump(empty($a['y']));\n\
             var_dump(empty($a['z']));\n\
             var_dump(empty($a['w']));\n\
             var_dump(empty($a['missing']));\n",
        VmOptions {
            collect_counters: true,
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(false)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\n"
    );
    let counters = result.counters.expect("counters should be collected");
    assert!(
        counters.record_shape_hits + counters.small_map_hits > 0,
        "empty() on a record-like/small-map array should consume the \
             fail-closed shape fast path: {counters:?}"
    );
}

#[test]
fn concat_prealloc_counters_cover_strings_scalars_and_object_fallbacks() {
    let string_scalar = execute_source_with_options(
        "<?php $a = 'a'; $b = 'b'; $value = 'id-' . 42; echo $value, '|', $a . $b;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(
        string_scalar.status.is_success(),
        "{:?}",
        string_scalar.status
    );
    assert_eq!(string_scalar.output.to_string_lossy(), "id-42|ab");
    let counters = string_scalar
        .counters
        .expect("counters should be collected");
    assert!(counters.concat_prealloc_hits >= 2, "{counters:?}");
    assert_eq!(
        counters.concat_fallback_by_reason.get("scalar_conversion"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("concat.scalar_conversion"),
        Some(&1),
        "{counters:?}"
    );

    let object = execute_source_with_options(
        "<?php
            class C {
                public function __toString(): string {
                    echo 'side|';
                    return 'object';
                }
            }
            echo new C() . '-tail';
            ",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(object.status.is_success(), "{:?}", object.status);
    assert_eq!(object.output.to_string_lossy(), "side|object-tail");
    let counters = object.counters.expect("counters should be collected");
    assert!(counters.concat_prealloc_hits >= 1, "{counters:?}");
    assert_eq!(
        counters.concat_fallback_by_reason.get("object_to_string"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("concat.object_to_string"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn environment_builtins_use_controlled_request_context() {
    let result = execute_source_with_options(
        "<?php
            echo getenv('PHP_APP_HOME'), \"\\n\";
            echo $_ENV['PHP_APP_CACHE_DIR'], \"\\n\";
            echo isset($_SERVER['argv']) ? 'argv' : 'missing', \"\\n\";
            echo $_SERVER['SCRIPT_NAME'], \"\\n\";
            echo php_sapi_name(), \"\\n\";
            echo php_uname('s'), '|', php_uname('n'), '|', php_uname('r'), \"\\n\";
            echo php_uname(), \"\\n\";
            echo get_current_user(), \"\\n\";
            putenv('PHP_APP_HOME=/changed');
            echo getenv('PHP_APP_HOME'), \"\\n\";
            putenv('PHP_APP_HOME');
            echo getenv('PHP_APP_HOME') === false ? 'unset' : 'bad', \"\\n\";
            try {
                putenv('=123');
            } catch (ValueError $exception) {
                echo $exception->getMessage(), \"\\n\";
            }
            try {
                putenv('');
            } catch (ValueError $exception) {
                echo $exception->getMessage(), \"\\n\";
            }
            ",
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                "/tmp/controlled.php",
                vec!["arg".to_string()],
            )
            .with_env(vec![
                ("PHP_APP_HOME".to_string(), "/tmp/php-app".to_string()),
                (
                    "PHP_APP_CACHE_DIR".to_string(),
                    "/tmp/php-app-cache".to_string(),
                ),
            ]),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        format!(
            "/tmp/php-app\n/tmp/php-app-cache\nargv\n/tmp/controlled.php\ncli\nPhrust|localhost|{}\nPhrust localhost {} Stdlib generic\nphrust\n/changed\nunset\nputenv(): Argument #1 ($assignment) must have a valid syntax\nputenv(): Argument #1 ($assignment) must have a valid syntax\n",
            php_source::reference_php_version(),
            php_source::reference_php_version()
        )
    );
}

#[test]
fn http_superglobals_are_visible_inside_user_functions() {
    let result = execute_source_with_options(
        "<?php function show_server() { echo is_array($_SERVER) ? $_SERVER['REQUEST_URI'] : 'bad'; } show_server();",
        VmOptions {
            runtime_context: RuntimeContext::controlled_http(
                php_runtime::api::RuntimeHttpRequestContext::new(
                    "GET",
                    "127.0.0.1:18080",
                    "/admin/install.php?step=0",
                    "/admin/install.php",
                    "/private/tmp/phrust-app-live/docroot/admin/install.php",
                    "/private/tmp/phrust-app-live/docroot",
                ),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "/admin/install.php?step=0");
}

#[test]
fn php_input_stream_reads_http_request_body_inside_vm_builtins() {
    let mut request = php_runtime::api::RuntimeHttpRequestContext::new(
        "POST",
        "127.0.0.1:18080",
        "/submit.php",
        "/submit.php",
        "/private/tmp/phrust-submit/submit.php",
        "/private/tmp/phrust-submit",
    );
    request.raw_body = b"raw=hello&n=2".to_vec().into();
    let result = execute_source_with_options(
        "<?php echo file_get_contents('php://input');",
        VmOptions {
            runtime_context: RuntimeContext::controlled_http(request),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"raw=hello&n=2");
}

#[test]
fn process_surface_is_disabled_by_default_without_crashing() {
    let result = execute_source(
        "<?php
            echo function_exists('shell_exec') ? 'has-shell|' : 'missing|';
            echo shell_exec('echo no') === false ? 'shell-disabled|' : 'bad|';
            echo exec('echo no') === false ? 'exec-disabled|' : 'bad|';
            echo system('echo no') === false ? 'system-disabled|' : 'bad|';
            echo passthru('echo no') === false ? 'passthru-disabled|' : 'bad|';
            $pipes = [];
            echo proc_open('echo no', [], $pipes) === false ? 'proc-disabled|' : 'bad|';
            echo popen('echo no', 'r') === false ? 'popen-disabled' : 'bad';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "has-shell|shell-disabled|exec-disabled|system-disabled|passthru-disabled|proc-disabled|popen-disabled"
    );
    assert_eq!(result.diagnostics.len(), 6);
    assert!(result.diagnostics.iter().all(|diagnostic| {
        diagnostic.id() == "E_PHP_VM_PROCESS_CAPABILITY_DISABLED"
            && diagnostic.severity() == RuntimeSeverity::Warning
    }));
}

#[test]
fn process_surface_can_use_isolated_mock_outputs() {
    let result = execute_source_with_options(
        "<?php
            echo shell_exec('echo mock');
            echo '|';
            $lines = [];
            $code = -1;
            echo exec('echo mock', $lines, $code), '|', $lines[0], ':', $lines[1], ':', $code, '|';
            echo system('echo mock'), '|';
            $passthruCode = -1;
            $pass = passthru('echo mock', $passthruCode);
            echo '|', $pass === null ? 'pass-null' : 'bad', ':', $passthruCode, '|';
            echo proc_get_status(false) === false ? 'proc-stub' : 'bad';
            ",
        VmOptions {
            runtime_context: RuntimeContext::default().with_process_mock("alpha\nbeta\n", 7),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "alpha\nbeta\n|beta|alpha:beta:7|alpha\nbeta\nbeta|alpha\nbeta\n|pass-null:7|proc-stub"
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_VM_PROCESS_RESOURCE_MOCK_UNSUPPORTED"
    );
}

#[test]
fn constants_execute_magic_constants_top_level_and_function() {
    let source = "<?php\nfunction f() {\n echo __FUNCTION__, \"|\", __LINE__, \"|\", __CLASS__, \"|\", __METHOD__, \"|\", __NAMESPACE__;\n}\necho __FILE__, \"|\", __DIR__, \"|\", __CLASS__, \"|\", __METHOD__, \"|\", __NAMESPACE__, \"\\n\";\nf();";
    let result = execute_source(source);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"/tmp/phrust-test.php|/tmp|||\nf|3||f|"
    );
}

#[test]
fn include_executes_local_file_and_returns_value() {
    let result = execute_fixture_file("fixtures/runtime/valid/includes/include-return.php");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"before|child:value|after\n");
}

#[test]
fn include_shares_top_level_locals() {
    let result = execute_fixture_file("fixtures/runtime/valid/includes/share-variable.php");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"parent|included\n");
}

#[test]
fn include_preserves_global_reference_slots_for_bootstrap_files() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-global-slots-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("version.php"),
        "<?php $required_php_version = '7.4';\n",
    )
    .expect("version include should be written");
    std::fs::write(
        root.join("load.php"),
        "<?php
            function check_runtime_versions() {
                global $required_php_version;
                echo $required_php_version, '|';
            }
            function redirect_if_needed() {
                echo $_SERVER['REQUEST_URI'];
            }
            ",
    )
    .expect("load include should be written");
    let source = "<?php
            require_once __DIR__ . '/version.php';
            require_once __DIR__ . '/load.php';
            check_runtime_versions();
            redirect_if_needed();
        ";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::controlled_http(
                php_runtime::api::RuntimeHttpRequestContext::new(
                    "POST",
                    "127.0.0.1:18080",
                    "/admin/install.php?step=2",
                    "/admin/install.php",
                    root.join("index.php").to_string_lossy(),
                    root.to_string_lossy(),
                ),
            )
            .with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7.4|/admin/install.php?step=2");
}

#[test]
fn include_global_statement_initializes_missing_global_to_null() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-global-null-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("plugin.php"),
        "<?php
            global $plugin_filter;
            echo $plugin_filter ? 'truthy' : 'falsey';
            ",
    )
    .expect("plugin include should be written");
    let source = "<?php require __DIR__ . '/plugin.php';";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"falsey");
}

#[test]
fn included_callback_can_call_entry_function_declared_later() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-entry-callback-visible-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("hook.php"),
        "<?php
            class AppHook {
                public static function run($callback) {
                    $callback();
                }
            }
            ",
    )
    .expect("hook include should be written");
    let source = "<?php
            include 'hook.php';
            AppHook::run('later_entry_callback');
            function later_entry_callback() {
                echo 'ok';
            }
        ";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn closure_array_cast_preserves_object_identity_for_spl_hash() {
    let result = execute_source(
        "<?php
            $callback = function () {};
            $array = (array) $callback;
            var_dump(is_object($array[0]));
            var_dump(spl_object_hash($array[0]) === spl_object_hash($callback));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"bool(true)\nbool(true)\n");
}

#[test]
fn include_declares_interface_before_main_class_implements_it() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-interface-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("contract.php"),
        "<?php interface I { function f($a = null); }\n",
    )
    .expect("contract include should be written");
    let source = "<?php
            include 'contract.php';
            class C implements I {
                function f($a = 2) {
                    var_dump($a);
                }
            }
            $c = new C;
            $c->f();
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"int(2)\n");
}

#[test]
fn param_type_checks_use_runtime_interfaces_from_includes() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-runtime-interface-type-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("contract.php"), "<?php interface I {}\n")
        .expect("contract include should be written");
    std::fs::write(
        root.join("implementation.php"),
        "<?php class C implements I {}\n",
    )
    .expect("implementation include should be written");
    let source = "<?php
            include 'contract.php';
            include 'implementation.php';
            function accept(?I $value): void {
                echo $value instanceof I ? 'ok' : 'bad';
            }
            accept(new C());
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn property_type_checks_use_runtime_interfaces_from_includes() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-runtime-interface-property-type-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("contract.php"), "<?php interface I {}\n")
        .expect("contract include should be written");
    std::fs::write(
        root.join("implementation.php"),
        "<?php class C implements I {}\n",
    )
    .expect("implementation include should be written");
    let source = "<?php
            include 'contract.php';
            include 'implementation.php';
            class Holder {
                public static ?I $value = null;
            }
            Holder::$value = new C();
            echo Holder::$value instanceof I ? 'ok' : 'bad';
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn include_declares_class_constants_before_initializer_reads_them() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-class-constant-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("constants.inc"),
        "<?php class A { const MY_CONST = 'hello from A'; }\n",
    )
    .expect("constant include should be written");
    let source = "<?php
            include 'constants.inc';
            class B {
                public static $a = A::MY_CONST;
                const ca = A::MY_CONST;
            }
            var_dump(B::$a);
            var_dump(B::ca);
        ";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("main.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"string(12) \"hello from A\"\nstring(12) \"hello from A\"\n"
    );
}

#[test]
fn comma_separated_class_constants_are_each_fetchable() {
    let result = execute_source(
        "<?php define('DYN', 123); class C { public const int A = 1, B = 2; const L = __LINE__; const F = __FILE__; const CL = __CLASS__; const D = DYN; } var_dump(C::A); var_dump(C::B); var_dump(C::L); var_dump(C::F); var_dump(C::CL); var_dump(C::D);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"int(1)\nint(2)\nint(1)\nstring(20) \"/tmp/phrust-test.php\"\nstring(1) \"C\"\nint(123)\n"
    );
}

#[test]
fn implemented_interface_constants_are_fetchable_through_class() {
    let result = execute_source(
        "<?php namespace SimplePie\\HTTP; interface Client { public const METHOD_GET = 'GET'; } final class FileClient implements Client { public static function check() { echo self::METHOD_GET, '|'; } } echo FileClient::METHOD_GET, '|'; FileClient::check();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"GET|GET|");
}

#[test]
fn missing_class_constant_initializer_fails_when_class_is_used() {
    let result =
        execute_source("<?php class C { const c1 = D::hello; } $a = new C(); echo 'unreachable';");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        result
            .output
            .to_string_lossy()
            .contains("Fatal error: Uncaught Error: Class \"D\" not found"),
        "{}",
        result.output.to_string_lossy()
    );
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("Class \"D\" not found")),
        "{:?}",
        result.status
    );
}

#[test]
fn include_once_and_require_once_skip_second_execution() {
    let result = execute_fixture_file("fixtures/runtime/valid/includes/include-once.php");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1\n");
}

#[test]
fn require_once_skips_file_loaded_by_plain_require() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-require-once-after-require-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("class.php"), "<?php class LoadedByRequire {}\n")
        .expect("class include should be written");
    std::fs::write(
        root.join("wrapper.php"),
        "<?php var_dump(require_once __DIR__ . '/class.php');\n",
    )
    .expect("wrapper include should be written");
    let source = "<?php
            require __DIR__ . '/class.php';
            require __DIR__ . '/wrapper.php';
            echo class_exists('LoadedByRequire', false) ? 'ok' : 'bad';
        ";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(cache),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"bool(true)\nok");
}

#[test]
fn include_cache_preserves_include_once_request_tracking() {
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let result = execute_fixture_file_with_options(
        "fixtures/runtime/valid/includes/include-once.php",
        VmOptions {
            include_cache: Some(Arc::clone(&cache)),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1\n");
    assert_eq!(cache.cache_stats().compile_misses, 1);
    assert!(cache.cache_stats().resolution_hits >= 2);
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.include_resolution_hits >= 2, "{counters:?}");
    assert_eq!(counters.include_compile_misses, 1, "{counters:?}");
    assert_eq!(counters.include_once_skips, 2, "{counters:?}");
}

#[test]
fn include_trace_records_cache_and_once_decisions() {
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let result = execute_fixture_file_with_options(
        "fixtures/runtime/valid/includes/include-once.php",
        VmOptions {
            include_cache: Some(Arc::clone(&cache)),
            trace_includes: true,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1\n");
    let events = include_trace_events(&result.trace);
    assert!(
        events
            .iter()
            .any(|event| event.contains("request kind=include_once")),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("resolution_cache=hit")),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("compile_cache=miss")),
        "{events:#?}"
    );
    assert!(
        events.iter().any(|event| event.contains("decision=skip")),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("entry_instructions=")),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("instructions_executed=")),
        "{events:#?}"
    );
}

#[test]
fn include_trace_records_repeated_normal_include_compile_cache_hits() {
    let root =
        std::env::temp_dir().join(format!("phrust-vm-include-repeat-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("common.php"),
        "<?php $count++; echo $count, '|';\n",
    )
    .expect("common include should be written");
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let result = execute_source_with_options_and_path(
        "<?php $count = 0; include 'common.php'; include './common.php'; echo $count, \"\\n\";",
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::clone(&cache)),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            trace_includes: true,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2|2\n");
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 1, "{stats:?}");
    assert!(stats.compile_hits >= 1, "{stats:?}");
    let events = include_trace_events(&result.trace);
    assert!(
        events
            .iter()
            .filter(|event| event.contains("execute-start kind=include"))
            .count()
            >= 2,
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("compile_cache=hit")),
        "{events:#?}"
    );
}

#[test]
fn anonymous_class_constructor_lookup_sees_parent_from_previous_include() {
    let root =
        std::env::temp_dir().join(format!("phrust-vm-anonymous-parent-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("parent.php"),
        "<?php class ParentForAnonymousProbe { public function __construct($value = null) {} }\n",
    )
    .expect("parent include should be written");
    std::fs::write(
            root.join("child.php"),
            "<?php function make_probe($value) { return new class($value) extends ParentForAnonymousProbe {}; }\n",
        )
        .expect("child include should be written");
    let source = "<?php require 'parent.php'; require 'child.php'; $object = make_probe('x'); echo $object instanceof ParentForAnonymousProbe ? 'yes' : 'no';";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"yes");
}

#[test]
fn inherited_property_defaults_use_declaring_include_unit_constants() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-inherited-property-defaults-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("parent.php"),
        "<?php class ParentDefaultProbe { public $registered = array(); public $group = 0; }\n",
    )
    .expect("parent include should be written");
    std::fs::write(
            root.join("child.php"),
            "<?php class ChildDefaultProbe extends ParentDefaultProbe { public $text_direction = 'ltr'; }\n",
        )
        .expect("child include should be written");
    let source = "<?php require 'parent.php'; require 'child.php'; $object = new ChildDefaultProbe(); echo gettype($object->registered), '|'; $object->registered['colors'] = true; echo count($object->registered), '|', $object->group;";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"array|1|0");
}

#[test]
fn anonymous_class_dependency_can_be_loaded_after_factory_include() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-anonymous-late-parent-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
            root.join("child.php"),
            "<?php function make_probe($value) { return new class($value) extends ParentAfterFactoryInclude { public function label() { return 'ok'; } }; }\n",
        )
        .expect("child include should be written");
    std::fs::write(
        root.join("parent.php"),
        "<?php class ParentAfterFactoryInclude { public function __construct($value = null) {} }\n",
    )
    .expect("parent include should be written");
    let source =
        "<?php require 'child.php'; require 'parent.php'; $object = make_probe('x'); echo 'ok';";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn anonymous_classes_from_different_includes_do_not_collide() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-anonymous-include-collision-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
            root.join("left.php"),
            "<?php function make_left_probe() { return new class { public function label() { return 'left'; } }; }\n",
        )
        .expect("left include should be written");
    std::fs::write(
            root.join("right.php"),
            "<?php function make_right_probe() { return new class { public function label() { return 'right'; } }; }\n",
        )
        .expect("right include should be written");
    let source = "<?php require 'left.php'; require 'right.php'; make_left_probe(); echo '|'; make_right_probe();";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"|");
}

#[test]
fn included_anonymous_class_object_property_iteration_uses_dynamic_class_table() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-anonymous-foreach-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("factory.php"),
        "<?php function make_foreach_probe() { return new class {}; }\n",
    )
    .expect("factory include should be written");
    let source = "<?php require 'factory.php'; $object = make_foreach_probe(); $object->a = 1; $object->b = 2; foreach ($object as $key => $value) { echo $key, '=', $value, '|'; }";
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a=1|b=2|");
}

#[test]
fn include_once_tracking_is_request_local_with_shared_compile_cache() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-once-request-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("once.php"), "<?php $count++;\n")
        .expect("once include should be written");
    let source = "<?php $count = 0; include_once 'once.php'; include_once './once.php'; echo $count, \"\\n\";";
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let options = || VmOptions {
        include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
        include_cache: Some(Arc::clone(&cache)),
        runtime_context: RuntimeContext::default().with_cwd(root.clone()),
        ..VmOptions::default()
    };
    let first = execute_source_with_options_and_path(
        source,
        options(),
        root.join("first.php").to_string_lossy().into_owned(),
    );
    let second = execute_source_with_options_and_path(
        source,
        options(),
        root.join("second.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(first.status.is_success(), "{:?}", first.status);
    assert!(second.status.is_success(), "{:?}", second.status);
    assert_eq!(first.output.as_bytes(), b"1\n");
    assert_eq!(second.output.as_bytes(), b"1\n");
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 1, "{stats:?}");
    assert!(stats.compile_hits >= 1, "{stats:?}");
}

#[test]
fn include_cache_keeps_globals_and_declarations_request_local() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-cache-request-state-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("lib.php"),
        "<?php
            $prompt07Marker = ($prompt07Marker ?? 0) + 1;
            echo 'include=', $prompt07Marker, '|';
            function prompt07_cached_value() { return 7; }
            ",
    )
    .expect("request-state include should be written");
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let options = || VmOptions {
        include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
        include_cache: Some(Arc::clone(&cache)),
        runtime_context: RuntimeContext::default().with_cwd(root.clone()),
        ..VmOptions::default()
    };
    let first = execute_source_with_options_and_path(
        "<?php include 'lib.php'; echo 'value=', prompt07_cached_value(), '|first';",
        options(),
        root.join("first.php").to_string_lossy().into_owned(),
    );
    let second = execute_source_with_options_and_path(
        "<?php include 'lib.php'; echo 'value=', prompt07_cached_value(), '|second';",
        options(),
        root.join("second.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(first.status.is_success(), "{:?}", first.status);
    assert!(second.status.is_success(), "{:?}", second.status);
    assert_eq!(first.output.as_bytes(), b"include=1|value=7|first");
    assert_eq!(second.output.as_bytes(), b"include=1|value=7|second");
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 1, "{stats:?}");
    assert!(stats.compile_hits >= 1, "{stats:?}");
}

#[test]
fn include_cache_invalidates_dynamic_declaration_strict_types_after_file_edit() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-include-cache-strict-edit-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let include = root.join("lib.php");
    std::fs::write(
            &include,
            "<?php function prompt07_rewritten_takes_int(int $value): void { echo 'weak=', $value; } prompt07_rewritten_takes_int('42');\n",
        )
        .expect("weak include should be written");
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let options = || VmOptions {
        include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
        include_cache: Some(Arc::clone(&cache)),
        runtime_context: RuntimeContext::default().with_cwd(root.clone()),
        ..VmOptions::default()
    };
    let source = "<?php include 'lib.php';";
    let weak = execute_source_with_options_and_path(
        source,
        options(),
        root.join("weak.php").to_string_lossy().into_owned(),
    );
    std::fs::write(
            &include,
            "<?php declare(strict_types=1); function prompt07_rewritten_takes_int(int $value): void { echo 'strict=', $value; } prompt07_rewritten_takes_int('42'); echo 'unreachable';\n",
        )
        .expect("strict include should be written");
    let strict = execute_source_with_options_and_path(
        source,
        options(),
        root.join("strict.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(weak.status.is_success(), "{:?}", weak.status);
    assert_eq!(weak.output.as_bytes(), b"weak=42");
    assert_eq!(strict.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        strict.output.to_string_lossy().contains("TypeError"),
        "{}",
        strict.output.to_string_lossy()
    );
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 2, "{stats:?}");
    assert!(stats.stale_invalidations >= 1, "{stats:?}");
}

#[test]
fn linked_trait_calls_use_the_declaring_files_strict_types_mode() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-linked-trait-strict-types-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("Traits")).expect("trait directory should be created");
    std::fs::write(
        root.join("Registry.php"),
        "<?php
            namespace Demo;
            use Demo\\Traits\\WeakCalls;
            use Demo\\Traits\\StrictCalls;
            function accepts_int(int $value): void { echo $value, '|'; }
            class Registry { use WeakCalls; use StrictCalls; }
        ",
    )
    .expect("registry should be written");
    std::fs::write(
        root.join("Traits/WeakCalls.php"),
        "<?php declare(strict_types=0); namespace Demo\\Traits;
            trait WeakCalls { public function weakCall(): void { \\Demo\\accepts_int('41'); } }
        ",
    )
    .expect("weak trait should be written");
    std::fs::write(
        root.join("Traits/StrictCalls.php"),
        "<?php declare(strict_types=1); namespace Demo\\Traits;
            trait StrictCalls { public function strictCall(): void { \\Demo\\accepts_int('42'); } }
        ",
    )
    .expect("strict trait should be written");
    let result = execute_source_with_options_and_path(
        "<?php include 'Registry.php'; $registry = new Demo\\Registry(); $registry->weakCall(); $registry->strictCall(); echo 'unreachable';",
        VmOptions {
            include_loader: Some(
                IncludeLoader::for_root(&root)
                    .expect("loader")
                    .with_compilation_dependency("Demo\\Traits\\WeakCalls", "Traits/WeakCalls.php")
                    .with_compilation_dependency(
                        "Demo\\Traits\\StrictCalls",
                        "Traits/StrictCalls.php",
                    ),
            ),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("main.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        result.output.to_string_lossy().starts_with("41|"),
        "{}",
        result.output.to_string_lossy()
    );
    assert!(
        result.output.to_string_lossy().contains("TypeError"),
        "{}",
        result.output.to_string_lossy()
    );
    assert!(!result.output.to_string_lossy().contains("unreachable"));
}

#[test]
fn linked_trait_files_preserve_execution_order_interfaces_and_reflection_paths() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-linked-trait-observability-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("Traits")).expect("trait directory should be created");
    std::fs::write(
        root.join("Registry.php"),
        "<?php namespace Demo;
            interface Contract { public function send(): string; }
            use Demo\\Traits\\PRIMARYTRAIT as First;
            use Demo\\Traits\\SecondaryTrait;
            echo 'root|';
            class Registry implements Contract {
                use First, SecondaryTrait {
                    First::send insteadof SecondaryTrait;
                    SecondaryTrait::send as backup;
                }
            }
        ",
    )
    .expect("registry should be written");
    let primary_path = root.join("Traits/PrimaryTrait.php");
    std::fs::write(
        &primary_path,
        "<?php namespace Demo\\Traits; echo 'primary|';
            trait PrimaryTrait { public function send(): string { return 'primary'; } }
        ",
    )
    .expect("primary trait should be written");
    let canonical_primary_path =
        std::fs::canonicalize(&primary_path).expect("primary trait path should canonicalize");
    std::fs::write(
        root.join("Traits/SecondaryTrait.php"),
        "<?php namespace Demo\\Traits; echo 'secondary|';
            trait SecondaryTrait { public function send(): string { return 'secondary'; } }
        ",
    )
    .expect("secondary trait should be written");
    let result = execute_source_with_options_and_path(
        "<?php include 'Registry.php';
            $registry = new Demo\\Registry();
            echo $registry->send(), '|', $registry->backup(), '|';
            echo count(get_included_files()), '|';
            echo (new ReflectionMethod(Demo\\Registry::class, 'send'))->getFileName();
        ",
        VmOptions {
            include_loader: Some(
                IncludeLoader::for_root(&root)
                    .expect("loader")
                    .with_compilation_dependency(
                        "Demo\\Traits\\PrimaryTrait",
                        "Traits/PrimaryTrait.php",
                    )
                    .with_compilation_dependency(
                        "Demo\\Traits\\SecondaryTrait",
                        "Traits/SecondaryTrait.php",
                    ),
            ),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("main.php").to_string_lossy().into_owned(),
    );
    let expected = format!(
        "primary|secondary|root|primary|secondary|4|{}",
        canonical_primary_path.display()
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), expected);
}

#[test]
fn include_trace_records_deep_finite_chain_without_recompilation() {
    let root = std::env::temp_dir().join(format!("phrust-vm-include-chain-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("a.php"), "<?php include 'b.php'; echo 'a|';\n")
        .expect("a include should be written");
    std::fs::write(root.join("b.php"), "<?php include 'c.php'; echo 'b|';\n")
        .expect("b include should be written");
    std::fs::write(root.join("c.php"), "<?php echo 'c|';\n").expect("c include should be written");
    let cache = Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let result = execute_source_with_options_and_path(
        "<?php include 'a.php'; include 'a.php'; echo \"done\\n\";",
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::clone(&cache)),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            trace_includes: true,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"c|b|a|c|b|a|done\n");
    let stats = cache.cache_stats();
    assert_eq!(stats.compile_misses, 3, "{stats:?}");
    assert!(stats.compile_hits >= 3, "{stats:?}");
    let events = include_trace_events(&result.trace);
    assert!(
        events.iter().any(|event| event.contains("stack_depth=3")),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("compile_cache=hit")),
        "{events:#?}"
    );
}

#[test]
fn include_trace_detects_recursive_cycle_with_stack() {
    let root = std::env::temp_dir().join(format!("phrust-vm-include-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("a.php"), "<?php require 'b.php';\n")
        .expect("a include should be written");
    std::fs::write(root.join("b.php"), "<?php require 'a.php';\n")
        .expect("b include should be written");
    let result = execute_source_with_options_and_path(
        "<?php require 'a.php'; echo 'after';",
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::new(IncludeCache::new_with_revalidation_interval(
                1,
                std::time::Duration::ZERO,
            ))),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            trace_includes: true,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(output.contains("E_PHP_VM_INCLUDE_CYCLE"), "{output}");
    assert!(output.contains("stack=["), "{output}");
}

#[test]
fn include_missing_warns_and_continues_but_require_missing_fails() {
    let include = execute_fixture_file_with_options(
        "fixtures/runtime/valid/includes/include-missing.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(include.status.is_success(), "{:?}", include.status);
    let include_output = String::from_utf8_lossy(include.output.as_bytes());
    assert!(
        include_output.starts_with("before|\nWarning: include("),
        "{include_output}"
    );
    assert!(
        include_output.contains("Failed to open stream: No such file or directory"),
        "{include_output}"
    );
    assert!(
        include_output.contains("Warning: include(): Failed opening '")
            && include_output.contains("' for inclusion (include_path='"),
        "{include_output}"
    );
    assert!(include_output.ends_with("after\n"), "{include_output}");
    assert_eq!(include.diagnostics[0].id(), "E_PHP_VM_INCLUDE_MISSING");
    assert_eq!(include.diagnostics[0].severity(), RuntimeSeverity::Warning);
    let Some(RuntimeDiagnosticPayload::IncludeFailure(payload)) = include.diagnostics[0].payload()
    else {
        panic!("missing include failure payload");
    };
    assert!(payload.target().ends_with("missing.php"), "{payload:?}");
    assert_eq!(payload.reason(), "No such file or directory");
    let counters = include.counters.expect("counters should be collected");
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("include_autoload.missing_path"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters.include_fallback_by_reason.get("missing_path"),
        Some(&1),
        "{counters:?}"
    );
    // A missing path is where a negative include cache would install an
    // entry; it stays disabled and records why.
    assert_eq!(
        counters
            .negative_include_cache_blocked_by_reason
            .get("directory_versions_unvalidated"),
        Some(&1),
        "{counters:?}"
    );

    let require = execute_fixture_file_with_options(
        "fixtures/runtime/invalid/includes/require-missing.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert_eq!(require.status.exit_status(), ExitStatus::RuntimeError);
    let require_output = String::from_utf8_lossy(require.output.as_bytes());
    assert!(
        require_output.starts_with("before|\nWarning: require("),
        "{require_output}"
    );
    assert!(
        require_output.contains("Failed to open stream: No such file or directory"),
        "{require_output}"
    );
    assert!(
        require_output.contains("\nFatal error: Uncaught Error: Failed opening required '"),
        "{require_output}"
    );
    assert_eq!(require.diagnostics[0].id(), "E_PHP_VM_INCLUDE_MISSING");
    assert_eq!(
        require.diagnostics[0].severity(),
        RuntimeSeverity::FatalError
    );
    let counters = require.counters.expect("counters should be collected");
    assert_eq!(
        counters
            .slow_path_calls_by_reason
            .get("include_autoload.missing_path"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters.include_fallback_by_reason.get("missing_path"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn negative_include_cache_preserves_missing_include_diagnostics() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-negative-include-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let loader = IncludeLoader::for_root(root.clone()).expect("loader");
    let cache = std::sync::Arc::new(IncludeCache::new_with_revalidation_interval(
        1,
        std::time::Duration::ZERO,
    ));
    let source =
        "<?php\necho 'a|';\ninclude 'nope.php';\necho 'b|';\ninclude 'nope.php';\necho 'c';";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            include_loader: Some(loader),
            include_cache: Some(std::sync::Arc::clone(&cache)),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    // Both failures present identically (modulo the include-site line number
    // the VM appends per site); the second is served from the
    // directory-version-guarded negative cache.
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert_eq!(
        output
            .matches("Failed to open stream: No such file or directory")
            .count(),
        2,
        "{output}"
    );
    assert_eq!(
        output.matches("Failed opening 'nope.php'").count(),
        2,
        "{output}"
    );
    let site_agnostic = |marker: &str| {
        output
            .split(marker)
            .nth(1)
            .and_then(|rest| rest.split(" on line ").next())
            .map(str::to_owned)
    };
    assert_eq!(
        site_agnostic("a|"),
        site_agnostic("b|"),
        "cached failure renders the same diagnostics: {output}"
    );
    assert!(output.ends_with('c'), "{output}");
    let stats = cache.cache_stats();
    assert_eq!(stats.negative_cache_installs, 1, "{stats:?}");
    assert_eq!(stats.negative_cache_hits, 1, "{stats:?}");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.negative_include_cache_hits, 1, "{counters:?}");
    assert_eq!(counters.negative_include_cache_installs, 1, "{counters:?}");

    // The file appearing invalidates the cached miss in the same process.
    std::fs::write(root.join("nope.php"), "<?php echo 'found';").expect("write include");
    let loader = IncludeLoader::for_root(root.clone()).expect("loader");
    let resolved = execute_source_with_options(
        "<?php include 'nope.php';",
        VmOptions {
            include_loader: Some(loader),
            include_cache: Some(std::sync::Arc::clone(&cache)),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    assert!(resolved.status.is_success(), "{:?}", resolved.status);
    assert_eq!(resolved.output.as_bytes(), b"found");
    assert_eq!(cache.cache_stats().negative_cache_invalidations, 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn export_persistent_function_callsites_reports_monomorphic_entry_unit_sites() {
    let source = "<?php function probe_tag(string $s): string { return $s . '!'; } $t = ''; for ($i = 0; $i < 64; $i++) { $t = probe_tag('x'); } echo $t;";
    let frontend = php_semantics::analyze_source(source);
    assert!(!frontend.has_errors());
    let lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_path: "/tmp/phrust-callsite-export.php".to_owned(),
            source_text: Some(source.to_owned()),
            ..php_ir::LoweringOptions::default()
        },
    );
    assert!(lowering.diagnostics.is_empty());
    let mut lowering = lowering;
    php_optimizer::PassPipeline::performance()
        .run(
            &mut lowering.unit,
            &php_optimizer::PassContext::new(php_optimizer::OptimizationLevel::O2),
        )
        .expect("optimizer");
    let vm = Vm::with_options(VmOptions {
        collect_counters: true,
        execution_format: ExecutionFormat::Auto,
        quickening: QuickeningMode::On,
        inline_caches: InlineCacheMode::On,
        superinstructions: SuperinstructionMode::On,
        jit: JitMode::Cranelift,
        ..VmOptions::default()
    });
    let result = vm.execute(lowering.unit);
    assert!(result.status.is_success(), "{:?}", result.status);
    let counters = result.counters.expect("counters");
    let sites = vm.export_persistent_function_callsites();
    assert_eq!(
        sites.len(),
        1,
        "fn_ic hits={} misses={} slots={} sites={sites:?}",
        counters.function_call_ic_hits,
        counters.function_call_ic_misses,
        counters.inline_cache_function_slots,
    );
    assert_eq!(sites[0].lowered_name, "probe_tag");
    assert_eq!(sites[0].arity, 1);
}

#[test]
fn composer_map_fingerprint_counters_attribute_presence_per_request() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-composer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let composer_dir = root.join("vendor").join("composer");
    std::fs::create_dir_all(&composer_dir).expect("create composer fixture");
    std::fs::write(
        composer_dir.join("autoload_classmap.php"),
        "<?php return [];\n",
    )
    .expect("write classmap");
    let script_path = root.join("index.php");
    // class_exists drives the autoload lookup cache, whose key carries the
    // Composer map fingerprint. Autoload lookups without a registered
    // callback preserve output either way — this is metadata + counters.
    let source = "<?php var_dump(class_exists('PhrustComposerProbeMissing'));";
    std::fs::write(&script_path, source).expect("write script");

    let with_map = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            runtime_context: RuntimeContext::controlled_cli(
                script_path.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_cwd(root.clone()),
            ..VmOptions::default()
        },
    );
    assert!(with_map.status.is_success(), "{:?}", with_map.status);
    assert_eq!(with_map.output.as_bytes(), b"bool(false)\n");
    let counters = with_map.counters.expect("counters");
    assert_eq!(counters.composer_fingerprint_present, 1, "{counters:?}");
    assert_eq!(counters.composer_fingerprint_missing, 0, "{counters:?}");

    let without_map = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            runtime_context: RuntimeContext::controlled_cli(
                "/nonexistent-phrust-root/index.php".to_owned(),
                Vec::new(),
            ),
            ..VmOptions::default()
        },
    );
    assert!(without_map.status.is_success(), "{:?}", without_map.status);
    assert_eq!(with_map.output, without_map.output);
    let counters = without_map.counters.expect("counters");
    assert_eq!(counters.composer_fingerprint_present, 0, "{counters:?}");
    assert_eq!(counters.composer_fingerprint_missing, 1, "{counters:?}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn builtin_context_persists_chdir_across_vm_builtin_calls() {
    let root =
        std::env::temp_dir().join(format!("phrust-vm-cwd-{}-{}", std::process::id(), "state"));
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("temp cwd should be created");
    let result = execute_source_with_options(
        "<?php
            echo basename(getcwd()), '|';
            var_dump(chdir('nested'));
            echo basename(getcwd()), '|';
            var_dump(chdir('..'));
            echo basename(getcwd());
            ",
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "phrust-vm-cwd-".to_owned()
            + &std::process::id().to_string()
            + "-state|bool(true)\nnested|bool(true)\nphrust-vm-cwd-"
            + &std::process::id().to_string()
            + "-state"
    );
}

#[test]
fn builtin_context_persists_stream_resources_across_vm_builtin_calls() {
    let root = std::env::temp_dir().join(format!("phrust-vm-stream-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp stream root should be created");
    let result = execute_source_with_options(
        "<?php
            $handle = fopen('data.txt', 'w+');
            echo is_resource($handle) ? 'resource|' : 'missing|';
            echo fwrite($handle, 'abc'), '|';
            rewind($handle);
            echo fread($handle, 3), '|';
            echo fclose($handle) ? 'closed' : 'open';
            ",
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"resource|3|abc|closed");
}

#[test]
fn builtin_context_persists_json_last_error_across_vm_builtin_calls() {
    let result = execute_source(
        "<?php
            var_dump(json_last_error());
            var_dump(json_last_error_msg());
            var_dump(json_decode('{'));
            var_dump(json_last_error());
            var_dump(json_last_error_msg());
            var_dump(json_decode('[]'));
            var_dump(json_last_error());
            var_dump(json_last_error_msg());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(0)\nstring(8) \"No error\"\nNULL\nint(4)\nstring(12) \"Syntax error\"\narray(0) {\n}\nint(0)\nstring(8) \"No error\"\n"
    );
}

#[test]
fn builtin_context_persists_preg_last_error_across_vm_builtin_calls() {
    let result = execute_source(
        "<?php
            var_dump(preg_last_error());
            var_dump(preg_last_error_msg());
            var_dump(preg_match('/[/', 'x'));
            var_dump(preg_last_error());
            var_dump(preg_last_error_msg());
            var_dump(preg_match('/x/', 'x'));
            var_dump(preg_last_error());
            var_dump(preg_last_error_msg());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with("int(0)\nstring(8) \"No error\"\n"),
        "{output}"
    );
    assert!(
        output.contains(
            "\nWarning: preg_match(): Compilation failed: missing terminating ] for character class at offset 1 in /tmp/phrust-test.php on line "
        ),
        "{output}"
    );
    assert!(
        output.ends_with(
            "bool(false)\nint(1)\nstring(14) \"Internal error\"\nint(1)\nint(0)\nstring(8) \"No error\"\n"
        ),
        "{output}"
    );
}

#[test]
fn builtin_context_persists_bcmath_scale_across_vm_builtin_calls() {
    let result = execute_source(
        "<?php
            var_dump(bcscale());
            echo bcadd('1.2', '3.45'), \"\\n\";
            var_dump(bcscale(3));
            var_dump(bcscale());
            echo bcadd('1.2', '3.45'), \"\\n\";
            echo bcadd('1.2', '3.45', 1), \"\\n\";
            var_dump(bcscale(0));
            var_dump(bcscale());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(0)\n4\nint(0)\nint(3)\n4.650\n4.6\nint(3)\nint(0)\n"
    );
}

#[test]
fn pcre_no_match_initializes_undefined_matches_with_later_flags() {
    let result = execute_source(
        "<?php
            var_dump(preg_match('/z/', 'abc', $matches, PREG_OFFSET_CAPTURE));
            var_dump($matches);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.output.to_string_lossy(), "int(0)\narray(0) {\n}\n");
}

#[test]
fn pcre_start_offset_ascii_word_fast_path_preserves_matches() {
    let result = execute_source(
        "<?php
            $str = str_repeat('a', 1024);
            $pos = 0;
            while (preg_match('/\\G\\w/u', $str, $matches, 0, $pos)) {
                ++$pos;
            }
            var_dump($pos);
            var_dump($matches);
            var_dump(preg_last_error());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(1024)\narray(0) {\n}\nint(0)\n"
    );
}

#[test]
fn dense_pcre_start_offset_fast_path_skips_post_call_discards() {
    let result = execute_source_with_options(
        "<?php
            $str = str_repeat('a', 2048);
            $pos = 0;
            while (preg_match('/\\G\\w/u', $str, $matches, 0, $pos)) {
                ++$pos;
            }
            var_dump($pos);
            var_dump($matches);
            ",
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(2048)\narray(0) {\n}\n"
    );
    let counters = result.counters.expect("counters");
    assert!(
        counters.opcodes.get("call_function").copied().unwrap_or(0) < 16,
        "{counters:?}"
    );
}

#[test]
fn dense_pcre_no_match_initializes_undefined_matches_without_warning() {
    let result = execute_source_with_options(
            "<?php
            function probe($file) {
                if (preg_match_all('#\\.\\./#', $file, $matches, PREG_SET_ORDER) && count($matches) > 1) {
                    echo 'bad';
                }
                echo isset($matches) ? count($matches) : 'missing';
            }
            probe('style.css');
            ",
            VmOptions {
                execution_format: ExecutionFormat::Auto,
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                ..VmOptions::default()
            },
        );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.output.to_string_lossy(), "0");
    let counters = result.counters.expect("counters");
    assert!(counters.bytecode_instructions_executed > 0, "{counters:?}");
}

#[test]
fn phar_supported_compression_follows_loaded_capabilities() {
    let result = execute_source(
        "<?php
            var_dump(extension_loaded('zlib'));
            var_dump(extension_loaded('bz2'));
            var_dump(Phar::getSupportedCompression());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nbool(false)\narray(1) {\n  [0]=>\n  string(2) \"GZ\"\n}\n"
    );
}

#[test]
fn phar_read_only_methods_dispatch_from_dense_runtime_path() {
    let root = std::env::temp_dir().join(format!(
        "phrust-phar-methods-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp phar root should be created");
    let archive = root.join("fixture-methods.phar");
    let archive_path = archive.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
$hex = '3c3f706870205f5f48414c545f434f4d50494c455228293b203f3e0a6b000000020000001101000000000c000000666978747572652e70686172000000000d0000006c69622f68656c6c6f2e7068702e000000800092652e00000000000000000000000000000008000000646174612e7478740700000080009265070000000000000000000000000000003c3f706870206563686f202766726f6d2d706861727c273b0a72657475726e2027696e636c7564652d6f6b273b0a7061796c6f6164';
file_put_contents('{archive_path}', hex2bin($hex));
$archive = new Phar('{archive_path}');
var_dump($archive->count());
var_dump($archive->offsetExists('data.txt'));
var_dump($archive->offsetExists('./lib/hello.php'));
var_dump($archive->offsetExists('missing.txt'));
var_dump(basename($archive->getPath()));
var_dump($archive->getAlias() !== '');
var_dump(str_contains($archive->getStub(), '__HALT_COMPILER'));
"#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(2)\nbool(true)\nbool(true)\nbool(false)\nstring(20) \"fixture-methods.phar\"\nbool(true)\nbool(true)\n"
    );
}

#[test]
fn phar_arrayaccess_returns_fileinfo_with_metadata() {
    let root = std::env::temp_dir().join(format!(
        "phrust-phar-arrayaccess-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp phar root should be created");
    let archive = root.join("metadata-fixture.phar");
    let archive_path = archive.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
$hex = '3c3f706870205f5f48414c545f434f4d50494c455228293b203f3e0d0abc00000002000000110000000100150000006d657461646174612d666978747572652e706861722b000000613a323a7b733a373a2261726368697665223b733a343a226d657461223b733a313a226e223b693a333b7d08000000646174612e74787407000000f93c506a07000000156a2c42a40100001d000000613a313a7b733a353a22656e747279223b733a343a226d657461223b7d0d0000006c69622f68656c6c6f2e7068702e000000f93c506a2e000000924eee49a4010000000000007061796c6f61643c3f706870206563686f202266726f6d2d706861727c223b2072657475726e2022696e636c7564652d6f6b223b0a84e76fd65c15ed5859574cf7d652aafa41b0259dc96873783289da7164e0dd0c0300000047424d42';
file_put_contents('{archive_path}', hex2bin($hex));
$archive = new Phar('{archive_path}');
$entry = $archive['data.txt'];
var_dump($archive instanceof ArrayAccess);
var_dump($archive instanceof Countable);
var_dump($entry instanceof PharFileInfo);
var_dump($entry instanceof SplFileInfo);
var_dump($archive->getMetadata());
var_dump($entry->getMetadata());
var_dump($entry->getContent());
var_dump($entry->getFilename());
echo str_replace('phar://{archive_path}/', '', $entry->getPathname()), "\n";
"#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\narray(2) {\n  [\"archive\"]=>\n  string(4) \"meta\"\n  [\"n\"]=>\n  int(3)\n}\narray(1) {\n  [\"entry\"]=>\n  string(4) \"meta\"\n}\nstring(7) \"payload\"\nstring(8) \"data.txt\"\ndata.txt\n"
    );
}

#[test]
fn phar_constructor_creates_empty_archive_and_inherits_file_info_methods() {
    let root = std::env::temp_dir().join(format!(
        "phrust-phar-empty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp phar root should be created");
    let archive = root.join("empty.phar.zip");
    let archive_path = archive.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $phar = new Phar("{archive_path}");
            var_dump($phar->isLink());
            var_dump($phar);
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                archive.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with("bool(false)\nobject(Phar)#"), "{output}");
    assert!(
        output.contains("[\"pathName\":\"SplFileInfo\":private]=>\n  string(0) \"\""),
        "{output}"
    );
    assert!(
        output.contains("[\"glob\":\"DirectoryIterator\":private]=>\n  bool(false)"),
        "{output}"
    );
    assert!(
        output.contains(
            "[\"subPathName\":\"RecursiveDirectoryIterator\":private]=>\n  string(0) \"\""
        ),
        "{output}"
    );
}

#[test]
fn builtin_context_persists_include_path_updates_for_stream_resolution() {
    let root = std::env::temp_dir().join(format!("phrust-vm-include-path-{}", std::process::id()));
    let includes = root.join("includes");
    std::fs::create_dir_all(&includes).expect("include dir should be created");
    std::fs::write(includes.join("target.php"), "<?php echo 'unused';\n")
        .expect("include target should be written");
    let include_path = includes.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        "<?php
            echo stream_resolve_include_path('target.php') === false ? 'missing|' : 'bad|';
            ini_set('include_path', '{include_path}');
            echo basename(stream_resolve_include_path('target.php')), '|';
            echo ini_get('include_path');
            "
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        format!("missing|target.php|{include_path}")
    );
}

#[test]
fn autoload_lookup_cache_preserves_composer_psr4_classmap_and_files() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate should live under workspace/crates/php_vm")
        .canonicalize()
        .expect("canonical workspace");
    let vendor = workspace.join("tests/fixtures/stdlib/composer/basic_project/vendor");
    let autoload = vendor.join("autoload.php");
    let source = format!(
            "<?php
            ini_set('include_path', '{}');
            require '{}';
            echo function_exists('stdlib_basic_file_helper') ? \"files-loaded\\n\" : \"files-missing\\n\";
            $psr = new Stdlib\\Basic\\PsrGreeter();
            echo $psr->message(), \"\\n\";
            $mapped = new Stdlib\\BasicClassmap\\MappedThing();
            echo $mapped->label(), \"\\n\";
            echo class_exists('Stdlib\\\\Basic\\\\Missing', true) ? \"bad\\n\" : \"safe-missing\\n\";
            ",
            vendor.display(),
            autoload.display()
        );
    let off = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(off.output, on.output);
    assert_eq!(
        on.output.as_bytes(),
        b"files-loaded\nfile-psr4\nfile-classmap\nsafe-missing\n"
    );
}

#[test]
fn autoload_lookup_cache_keeps_files_autoload_side_effects_visible() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate should live under workspace/crates/php_vm")
        .canonicalize()
        .expect("canonical workspace");
    let vendor = workspace.join("tests/fixtures/stdlib/composer/basic_project/vendor");
    let autoload = vendor.join("autoload.php");
    let source = format!(
            "<?php
            ini_set('include_path', '{}');
            include_once '{}';
            include_once '{}';
            echo function_exists('stdlib_basic_file_helper') ? \"files-first\\n\" : \"files-missing\\n\";
            echo stdlib_basic_file_helper('order'), \"\\n\";
            $psr = new Stdlib\\Basic\\PsrGreeter();
            echo $psr->message(), \"\\n\";
            $mapped = new Stdlib\\BasicClassmap\\MappedThing();
            echo $mapped->label(), \"\\n\";
            echo count(spl_autoload_functions()), \"\\n\";
            echo class_exists('Stdlib\\\\Basic\\\\Missing', true) ? \"bad\\n\" : \"safe-missing\\n\";
            ",
            vendor.display(),
            autoload.display(),
            autoload.display()
        );
    let off = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(off.output, on.output);
    assert_eq!(
        on.output.as_bytes(),
        b"files-first\nfile-order\nfile-psr4\nfile-classmap\n1\nsafe-missing\n"
    );
}

#[test]
fn autoload_lookup_cache_records_hits_and_keeps_missing_autoload_side_effects() {
    let source = "<?php
            $autoloadCount = 0;
            function perf_counting_loader($class) {
                global $autoloadCount;
                $autoloadCount = $autoloadCount + 1;
            }
            spl_autoload_register('perf_counting_loader');
            for ($i = 0; $i < 2; $i++) {
                if (class_exists('PerfMissingSideEffect', true)) {
                    echo 'bad';
                } else {
                    echo 'miss';
                }
            }
            echo ':', $autoloadCount, \"\\n\";
            class PerfPositiveCache {}
            for ($i = 0; $i < 3; $i++) {
                if (class_exists('PerfPositiveCache', false)) {
                    echo 'hit';
                } else {
                    echo 'bad';
                }
            }
            ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "missmiss:2\nhithithit");
    let counters = result.counters.expect("counters");
    assert!(counters.autoload_class_lookup_ic_hits > 0, "{counters:?}");
    assert!(counters.autoload_class_lookup_ic_misses > 0, "{counters:?}");
    assert!(counters.autoload_graph_hits > 0, "{counters:?}");
    assert!(counters.autoload_graph_misses > 0, "{counters:?}");
    assert_eq!(counters.negative_lookup_hits, 0, "{counters:?}");
}

#[test]
fn autoload_lookup_cache_records_negative_hits_without_side_effects() {
    let source = "<?php
            function perf_missing_no_autoload() {
                return class_exists('PerfNegativeCacheMissing', false);
            }
            for ($i = 0; $i < 4; $i++) {
                echo perf_missing_no_autoload() ? 'bad' : 'miss';
            }
            ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "missmissmissmiss");
    let counters = result.counters.expect("counters");
    assert!(counters.autoload_class_lookup_ic_hits > 0, "{counters:?}");
    assert!(counters.autoload_class_lookup_ic_misses > 0, "{counters:?}");
    assert!(counters.autoload_graph_hits > 0, "{counters:?}");
    assert!(counters.autoload_graph_misses > 0, "{counters:?}");
    assert!(counters.negative_lookup_hits > 0, "{counters:?}");
}

#[test]
fn autoload_lookup_cache_invalidates_after_spl_autoload_register() {
    std::thread::Builder::new()
        .name("autoload_lookup_cache_invalidates_after_spl_autoload_register".to_owned())
        // Match the harness default (RUST_MIN_STACK=32MiB): the debug
        // build's execute_function frame is ~2MiB, so an 8MiB cap sat one
        // nested include away from overflow.
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .expect("crate should live under workspace/crates/php_vm")
                .canonicalize()
                .expect("canonical workspace");
            let class_file =
                workspace.join("tests/fixtures/performance/inline_cache/PerfRegisteredCache.php");
            let source = "<?php
                    function perf_registered_exists() {
                        return class_exists('PerfRegisteredCache', true);
                    }
                    if (perf_registered_exists()) {
                        echo 'bad';
                    } else {
                        echo 'miss';
                    }
                    spl_autoload_register(function ($class) {
                        if (strtolower($class) === 'perfregisteredcache') {
                            include '__CLASS_FILE__';
                        }
                    });
                    if (perf_registered_exists()) {
                        echo ':hit';
                    } else {
                        echo ':bad';
                    }
                    "
            .replace("__CLASS_FILE__", &class_file.to_string_lossy());
            let result = execute_source_with_options(
                &source,
                VmOptions {
                    include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
                    collect_counters: true,
                    collect_profile_spans: false,
                    collect_layout_source_attribution: true,
                    inline_caches: InlineCacheMode::On,
                    ..VmOptions::default()
                },
            );

            assert!(result.status.is_success(), "{:?}", result.status);
            assert_eq!(result.output.to_string_lossy(), "miss:hit");
            let counters = result.counters.expect("counters");
            assert!(
                counters.autoload_class_lookup_ic_invalidations > 0,
                "{counters:?}"
            );
        })
        .expect("autoload test thread should spawn")
        .join()
        .expect("autoload test thread should finish");
}

#[test]
fn autoload_lookup_cache_invalidates_negative_after_new_include() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate should live under workspace/crates/php_vm")
        .canonicalize()
        .expect("canonical workspace");
    let class_file =
        workspace.join("tests/fixtures/performance/inline_cache/PerfIncludedCache.php");
    let source = "<?php
            function perf_included_exists() {
                return class_exists('PerfIncludedCache', false);
            }
            if (perf_included_exists()) {
                echo 'bad';
            } else {
                echo 'miss';
            }
            include '__CLASS_FILE__';
            if (perf_included_exists()) {
                echo ':hit';
            } else {
                echo ':bad';
            }
            "
    .replace("__CLASS_FILE__", &class_file.to_string_lossy());
    let result = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&workspace).expect("loader")),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "miss:hit");
    let counters = result.counters.expect("counters");
    assert!(
        counters.autoload_class_lookup_ic_invalidations > 0,
        "{counters:?}"
    );
}

#[test]
fn include_path_inline_cache_records_hits_and_preserves_semantics() {
    let off = execute_fixture_file_with_options(
        "tests/fixtures/performance/inline_cache/include-path-cache.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_fixture_file_with_options(
        "tests/fixtures/performance/inline_cache/include-path-cache.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(off.output, on.output);
    assert_eq!(on.output.as_bytes(), b"VVVOnce\n");
    assert_eq!(
        off.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.id())
            .collect::<Vec<_>>(),
        on.diagnostics
            .iter()
            .map(|diagnostic| diagnostic.id())
            .collect::<Vec<_>>()
    );
    let counters = on.counters.expect("counters");
    assert!(counters.inline_cache_include_path_slots > 0, "{counters:?}");
    assert!(counters.include_path_ic_hits > 0, "{counters:?}");
    assert!(counters.include_path_ic_misses > 0, "{counters:?}");
    assert!(counters.include_graph_hits > 0, "{counters:?}");
    assert!(counters.include_graph_misses > 0, "{counters:?}");
    // Every IC revalidation also observes the parent-directory version
    // (metadata only): a stable fixture directory always matches.
    assert!(counters.directory_version_hits > 0, "{counters:?}");
    assert_eq!(counters.directory_version_misses, 0, "{counters:?}");
}

#[test]
fn include_path_inline_cache_preserves_include_path_order() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate should live under workspace/crates/php_vm");
    let lib_root = workspace.join("tests/fixtures/performance/inline_cache/include-path-cache-lib");
    let first = lib_root.join("first");
    let second = lib_root.join("second");
    let source = "<?php include 'chosen.php';";
    let first_result = execute_source_with_options(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(lib_root.clone()).expect("loader")),
            runtime_context: RuntimeContext::default()
                .with_include_path(vec![first.clone(), second.clone()]),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    let second_result = execute_source_with_options(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(lib_root).expect("loader")),
            runtime_context: RuntimeContext::default().with_include_path(vec![second, first]),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(
        first_result.status.is_success(),
        "{:?}",
        first_result.status
    );
    assert!(
        second_result.status.is_success(),
        "{:?}",
        second_result.status
    );
    assert_eq!(first_result.output.as_bytes(), b"First");
    assert_eq!(second_result.output.as_bytes(), b"Second");
}

#[test]
fn include_path_inline_cache_preserves_missing_file_warning() {
    let off = execute_fixture_file_with_options(
        "fixtures/runtime/valid/includes/include-missing.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_fixture_file_with_options(
        "fixtures/runtime/valid/includes/include-missing.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(off.output, on.output);
    assert_eq!(off.diagnostics.len(), 1);
    assert_eq!(on.diagnostics.len(), 1);
    assert_eq!(off.diagnostics[0].id(), on.diagnostics[0].id());
    assert_eq!(off.diagnostics[0].severity(), on.diagnostics[0].severity());
    let counters = on.counters.expect("counters");
    assert_eq!(
        counters.fallback_by_path_semantics.get("missing_path"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn include_path_inline_cache_invalidates_changed_file_metadata() {
    let root = std::env::temp_dir().join(format!("phrust-include-path-ic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    let include_path = root.join("mutable.php");
    std::fs::write(&include_path, "<?php echo 'A';\n").expect("write include");
    let loader = IncludeLoader::for_root(root.clone()).expect("loader");
    let resolved = loader
        .resolve_with_include_path(None, &include_path.to_string_lossy(), &[], None)
        .expect("resolve include");
    let request = IncludePathCacheKey {
        path: include_path.to_string_lossy().into_owned(),
        include_path: Vec::new(),
        cwd: root.clone(),
        calling_file_directory: None,
    };
    let mut table = InlineCacheTable::default();
    let function = FunctionId::new(0);
    let block = BlockId::new(0);
    let instruction = InstrId::new(0);
    table.observe_slot(
        23,
        function,
        block,
        instruction,
        InlineCacheKind::IncludePath,
    );
    table.install_include_path(
        23,
        function,
        block,
        instruction,
        request.clone(),
        InvalidationEpoch::new(1),
        IncludePathCacheTarget {
            canonical_path: resolved.canonical_path.clone(),
            resolution_path: resolved.resolution_path.clone(),
            fingerprint: resolved.fingerprint.clone(),
            directory_version: resolved.directory_version,
        },
    );
    std::fs::write(&include_path, "<?php echo 'changed';\n").expect("rewrite include");
    let (target, probe) = table.lookup_include_path(
        23,
        function,
        block,
        instruction,
        &request,
        InvalidationEpoch::new(1),
    );
    let target = target.expect("cached target");
    let event = if target.is_current() {
        table.record_include_path_hit(23, function, block, instruction)
    } else {
        table.invalidate_include_path(23, function, block, instruction)
    };
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(probe.kind, Some(InlineCacheKind::IncludePath));
    assert!(event.invalidation);
    assert!(event.miss);
}

#[cfg(unix)]
#[test]
fn include_path_inline_cache_rejects_symlink_target_swap() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "phrust-include-path-ic-link-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::fs::write(root.join("first.php"), "<?php echo 'first';\n").expect("first target");
    std::fs::write(root.join("other.php"), "<?php echo 'other';\n").expect("other target");
    let link = root.join("linked.php");
    symlink(root.join("first.php"), &link).expect("first symlink");
    let loader = IncludeLoader::for_root(&root).expect("loader");
    let resolved = loader
        .resolve_with_include_path(None, &link.to_string_lossy(), &[], None)
        .expect("resolve first symlink target");
    let target = IncludePathCacheTarget::from_resolved(&resolved);
    assert!(target.is_current());

    std::fs::remove_file(&link).expect("remove first symlink");
    symlink(root.join("other.php"), &link).expect("replacement symlink");

    assert!(!target.is_current());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn include_path_graph_invalidates_changed_file_metadata_in_vm() {
    let root = std::env::temp_dir().join(format!(
        "phrust-include-path-graph-vm-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    let include_path = root.join("mutable.php");
    std::fs::write(&include_path, "<?php echo 'A';\n").expect("write include");
    let include_path_php = include_path.to_string_lossy().replace('\\', "\\\\");
    let replacement = "<?php echo 'B';\n"
        .replace('\\', "\\\\")
        .replace('\'', "\\'");
    let source = format!(
        "<?php
            function perf_load_mutable() {{
                include '{include_path_php}';
            }}
            perf_load_mutable();
            file_put_contents('{include_path_php}', '{replacement}');
            perf_load_mutable();
            "
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(root.clone()).expect("loader")),
            runtime_context: RuntimeContext::default()
                .with_cwd(root.clone())
                .with_filesystem_capabilities(
                    php_runtime::api::FilesystemCapabilities::none()
                        .with_allowed_roots(vec![root.clone()]),
                ),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "AB");
    let counters = result.counters.expect("counters");
    assert!(counters.include_path_ic_invalidations > 0, "{counters:?}");
    assert_eq!(
        counters
            .invalidations_by_reason
            .get("file_fingerprint_changed"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn objects_execute_constructor_and_public_properties() {
    let result = execute_source(
        "<?php class Box { public $value; function __construct($value) { $this->value = $value; } } $box = new Box(7); echo $box->value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");

    let constructorless_args = execute_source(
        "<?php class NoCtor {} $direct = new NoCtor('ignored'); $name = 'NoCtor'; $dynamic = new $name('also ignored'); echo $direct::class, '|', $dynamic::class;",
    );
    assert!(
        constructorless_args.status.is_success(),
        "{:?}",
        constructorless_args.status
    );
    assert_eq!(constructorless_args.output.as_bytes(), b"NoCtor|NoCtor");

    let magic_after_unset = execute_source(
        "<?php class Box { public $value = 1; public function __get($name) { return $name . ':magic'; } } $box = new Box(); unset($box->value); echo $box->value;",
    );
    assert!(
        magic_after_unset.status.is_success(),
        "{:?}",
        magic_after_unset.status
    );
    assert_eq!(magic_after_unset.output.as_bytes(), b"value:magic");

    let magic_private_dynamic = execute_source(
        "<?php class BlockTypeProbe { private $uses_context = array('postId'); public function __isset($name) { echo 'isset:' . $name . '|'; return $name === 'uses_context'; } public function __get($name) { echo 'get:' . $name . '|'; return 'magic:' . $name; } } $field = 'uses_context'; $probe = new BlockTypeProbe(); echo isset($probe->{$field}) ? 'yes|' : 'no|'; echo $probe->{$field};",
    );
    assert!(
        magic_private_dynamic.status.is_success(),
        "{:?}\n{}",
        magic_private_dynamic.status,
        magic_private_dynamic.output.to_string_lossy()
    );
    assert_eq!(
        magic_private_dynamic.output.as_bytes(),
        b"isset:uses_context|yes|get:uses_context|magic:uses_context"
    );

    let magic_set_after_unset = execute_source(
        "<?php class Box { public $value = 1; public function __set($name, $value) { echo $name, ':set|'; $this->$name = $value; } } $box = new Box(); unset($box->value); $box->value = 3; echo $box->value;",
    );
    assert!(
        magic_set_after_unset.status.is_success(),
        "{:?}",
        magic_set_after_unset.status
    );
    assert_eq!(magic_set_after_unset.output.as_bytes(), b"value:set|3");

    let magic_set_dynamic = execute_source(
        "<?php class Box { public function __set($name, $value) { $this->$name = $value; } } $box = new Box(); $box->missing = 4; echo $box->missing;",
    );
    assert!(
        magic_set_dynamic.status.is_success(),
        "{:?}",
        magic_set_dynamic.status
    );
    assert_eq!(magic_set_dynamic.output.as_bytes(), b"4");

    let null_after_unset = execute_source(
        "<?php class Box { public $value = 1; } $box = new Box(); unset($box->value); echo $box->value === null ? 'null' : 'value';",
    );
    assert!(
        null_after_unset.status.is_success(),
        "{:?}",
        null_after_unset.status
    );
    let output = null_after_unset.output.to_string_lossy();
    assert!(output.contains("Warning: Undefined property: Box::$value in "));
    assert!(output.trim_end().ends_with("null"), "{output}");
    assert_eq!(
        null_after_unset.diagnostics[0].id(),
        "E_PHP_VM_UNDEFINED_PROPERTY"
    );
}

#[test]
fn foreach_over_null_warns_and_iterates_zero_times() {
    let result = execute_source(
        "<?php class Foo { function __destruct() { foreach ($this->x as $x); } } new Foo(); echo 'OK';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Warning: Undefined property: Foo::$x in "),
        "{output}"
    );
    assert!(
        output.contains("Warning: foreach() argument must be of type array|object, null given in "),
        "{output}"
    );
    assert!(output.trim_end().ends_with("OK"), "{output}");
    let diagnostic_ids: Vec<_> = result
        .diagnostics
        .iter()
        .map(RuntimeDiagnostic::id)
        .collect();
    assert!(
        diagnostic_ids.contains(&"E_PHP_VM_FOREACH_INVALID_SOURCE"),
        "{diagnostic_ids:?}"
    );
}

#[test]
fn objects_keep_independent_instance_properties() {
    let result = execute_source(
        "<?php class Cell { public $value; } $left = new Cell(); $right = new Cell(); $left->value = 1; $right->value = 2; echo $left->value, '|', $right->value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2");
}

#[test]
fn objects_report_unknown_class_and_unsupported_property_modifier() {
    let unknown = execute_source("<?php $object = new MissingObject();");

    assert_eq!(unknown.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(unknown.diagnostics[0].id(), "E_PHP_VM_UNKNOWN_CLASS");
    let context = first_runtime_bringup_payload(&unknown).fields();
    assert_eq!(
        context.get("bringup_error_class").map(String::as_str),
        Some("autoload_lookup")
    );
    assert_eq!(
        context.get("requested_name").map(String::as_str),
        Some("missingobject")
    );
    assert_eq!(
        context.get("normalized_name").map(String::as_str),
        Some("missingobject")
    );
    assert_eq!(
        context.get("lookup_kind").map(String::as_str),
        Some("class")
    );
    assert_eq!(
        context.get("autoload_enabled").map(String::as_str),
        Some("true")
    );
    assert!(context.contains_key("class_table_epoch"), "{context:?}");
    assert!(context.contains_key("autoload_stack_epoch"), "{context:?}");

    let private = execute_source(
        "<?php class PrivateSlot { private $value; function set($value) { $this->value = $value; } function get() { return $this->value; } } $slot = new PrivateSlot(); $slot->set(4); echo $slot->get();",
    );
    assert!(private.status.is_success(), "{:?}", private.status);
    assert_eq!(private.output.as_bytes(), b"4");
}

#[test]
fn bringup_diagnostics_classify_callable_and_builtin_failures() {
    let callable = execute_source("<?php array_map('missing_callback', [1]);");
    assert_eq!(callable.status.exit_status(), ExitStatus::RuntimeError);
    let callable_context = first_runtime_bringup_payload(&callable).fields();
    assert_eq!(
        callable_context
            .get("bringup_error_class")
            .map(String::as_str),
        Some("callable_resolution")
    );
    assert_eq!(
        callable_context.get("requested_name").map(String::as_str),
        Some("missing_callback")
    );
    assert_eq!(
        callable_context.get("lookup_kind").map(String::as_str),
        Some("function")
    );

    // Undefined functions are catchable Errors now, so the bring-up payload
    // is replaced by the uncaught-exception surface when nothing catches.
    let builtin = execute_source("<?php missing_app_function();");
    assert_eq!(builtin.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(builtin.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        builtin.diagnostics[0]
            .message()
            .contains("Uncaught Error: Call to undefined function missing_app_function()"),
        "{}",
        builtin.diagnostics[0].message()
    );
}

#[test]
fn dynamic_instanceof_uses_runtime_class_and_interface_names() {
    let result = execute_source(
        "<?php interface A {} interface B extends A {} class C implements B {} $object = new C(); $a = 'A'; $b = 'B'; $c = 'C'; $missing = 'Missing'; if ($object instanceof $a) { echo 'a|'; } if ($object instanceof $b) { echo 'b|'; } if ($object instanceof $c) { echo 'c|'; } if ($object instanceof $missing) { echo 'missing'; } else { echo 'no'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a|b|c|no");
}

#[test]
fn logical_not_binds_after_instanceof_but_before_comparison() {
    let result = execute_source(
        "<?php
            class ContentPost { public $filter = 'raw'; }
            $post = 123;
            echo (! $post instanceof ContentPost || ! isset($post->filter)) ? 'short|' : 'bad|';
            $post = new ContentPost();
            echo (! $post instanceof ContentPost || ! isset($post->filter)) ? 'bad|' : 'object|';
            echo (! 1 < 2) ? 'comparison' : 'bad';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"short|object|comparison");
}

#[test]
fn clone_executes_shallow_object_copy_with_independent_identity() {
    let result = execute_source(
        "<?php class Cell { public $value; } $original = new Cell(); $original->value = 1; $copy = clone $original; $copy->value = 2; echo $original->value, '|', $copy->value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2");
}

#[test]
fn static_properties_accessed_as_instance_slots_emit_notices() {
    let result = execute_source(
        "<?php error_reporting(2047); class MyCloneable { static $id = 0; function __construct() { $this->id = self::$id++; } function __clone() { $this->id = self::$id++; } } $original = new MyCloneable(); echo $original->id, '|'; $clone = clone $original; echo $clone->id;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("0|"), "{output}");
    assert!(output.trim_end().ends_with('1'), "{output}");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.id() == "E_PHP_VM_STATIC_PROPERTY_AS_NON_STATIC_NOTICE"
            })
            .count(),
        4
    );
}

#[test]
fn dynamic_object_class_name_and_clone_visibility_errors_are_catchable() {
    let class_name = execute_source(
        "<?php try { throw new Error('boom'); } catch (Throwable $e) { echo $e::class, ':', $e->getMessage(); }",
    );
    assert!(class_name.status.is_success(), "{:?}", class_name.status);
    assert_eq!(class_name.output.as_bytes(), b"Error:boom");

    let protected_clone = execute_source(
        "<?php class test { protected function __clone() {} } try { $obj = new test; $clone = clone $obj; } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(); }",
    );
    assert!(
        protected_clone.status.is_success(),
        "{:?}",
        protected_clone.status
    );
    assert_eq!(
        protected_clone.output.as_bytes(),
        b"Error: Call to protected method test::__clone() from global scope"
    );

    let private_clone = execute_source(
        "<?php class test { private function __clone() {} } try { $obj = new test; $clone = clone $obj; } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(); }",
    );
    assert!(
        private_clone.status.is_success(),
        "{:?}",
        private_clone.status
    );
    assert_eq!(
        private_clone.output.as_bytes(),
        b"Error: Call to private method test::__clone() from global scope"
    );
}

#[test]
fn constructor_visibility_distinguishes_new_and_scoped_parent_call() {
    let private_new = execute_source(
        "<?php class test { private function __construct() {} } try { new test(1); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(); }",
    );
    assert!(private_new.status.is_success(), "{:?}", private_new.status);
    assert_eq!(
        private_new.output.as_bytes(),
        b"Error: Call to private test::__construct() from global scope"
    );

    let parent_private = execute_source(
        "<?php class BaseCtor { private function __construct() {} } class ChildCtor extends BaseCtor { public function __construct() { parent::__construct(); } } try { new ChildCtor(); } catch (Throwable $e) { echo $e::class, ': ', $e->getMessage(); }",
    );
    assert!(
        parent_private.status.is_success(),
        "{:?}",
        parent_private.status
    );
    assert_eq!(
        parent_private.output.as_bytes(),
        b"Error: Cannot call private BaseCtor::__construct()"
    );
}

#[test]
fn userland_exception_subclass_can_call_parent_constructor() {
    let result = execute_source(
        "<?php class HttpException extends Exception { public $type; public function __construct($message, $type, $code = 0) { parent::__construct($message, $code); $this->type = $type; } } $e = new HttpException('blocked', 'network', 7); echo $e->getMessage(), '|', $e->getCode(), '|', $e->type, '|', ($e instanceof Exception ? 'yes' : 'no');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"blocked|7|network|yes");
}

#[test]
fn direct_internal_exception_constructor_preserves_code() {
    let result = execute_source(
        "<?php $e = new Exception('blocked', 99); echo $e->getMessage(), '|', $e->getCode();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"blocked|99");
}

#[test]
fn userland_exception_subclass_inherits_throwable_methods_for_callables() {
    let result = execute_source(
        "<?php class CallbackException extends Exception { public function __construct($message) { parent::__construct($message); } } $e = new CallbackException('callable'); echo call_user_func([$e, 'getMessage']);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"callable");
}

#[test]
fn destructors_run_at_shutdown_in_reverse_registration_order() {
    let result = execute_source(
        "<?php class D { public $name; function __construct($name) { $this->name = $name; } function __destruct() { echo 'd:', $this->name, '|'; } } $a = new D('a'); $b = new D('b'); echo 'body|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"body|d:b|d:a|");
}

#[test]
fn shutdown_stages_append_to_the_single_final_output_buffer() {
    let result = execute_source(
        "<?php
        class ShutdownOutputObject {
            function __destruct() { echo '|destructor'; }
        }
        function shutdown_output() { echo '|shutdown'; }
        $object = new ShutdownOutputObject();
        register_shutdown_function('shutdown_output');
        echo str_repeat('x', 65536);
        ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.len(), 65_536 + 20);
    assert!(
        result
            .output
            .to_string_lossy()
            .ends_with("|shutdown|destructor")
    );
}

#[test]
fn included_class_destructor_runs_from_declaring_unit_at_shutdown() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-included-destructor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let dependency_path = root.join("dep.php");
    std::fs::write(
        &dependency_path,
        "<?php class IncludedDestructor { public function __destruct() { echo 'destruct|'; } }",
    )
    .expect("dependency should be writable");
    let main_path = root.join("main.php");
    let source =
        "<?php require __DIR__ . '/dep.php'; $keep = new IncludedDestructor(); echo 'body|';";
    std::fs::write(&main_path, source).expect("main source should be writable");
    let loader = IncludeLoader::for_root(root.clone()).expect("include loader");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(loader),
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );

    let _ = std::fs::remove_file(dependency_path);
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_dir(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"body|destruct|");
}

#[test]
fn included_class_clone_resolves_dynamic_object_class_case_insensitively() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-included-clone-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let dependency_path = root.join("dep.php");
    std::fs::write(
            &dependency_path,
            "<?php class BlogPost { public $x; } return unserialize('O:8:\"blogpost\":1:{s:1:\"x\";i:4;}');",
        )
        .expect("dependency should be writable");
    let main_path = root.join("main.php");
    let source = "<?php $post = require __DIR__ . '/dep.php'; $copy = clone $post; echo get_class($copy), '|', $copy->x;";
    std::fs::write(&main_path, source).expect("main source should be writable");
    let loader = IncludeLoader::for_root(root.clone()).expect("include loader");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(loader),
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );

    let _ = std::fs::remove_file(dependency_path);
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_dir(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"BlogPost|4");
}

#[test]
fn clone_of_included_child_resolves_parent_from_dynamic_state() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-included-child-clone-parent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let parent_path = root.join("parent.php");
    std::fs::write(
        &parent_path,
        "<?php class CloneParentProbe { public $parent = 'parent'; }",
    )
    .expect("parent source should be writable");
    let child_path = root.join("child.php");
    std::fs::write(
            &child_path,
            "<?php require_once __DIR__ . '/parent.php'; class CloneChildProbe extends CloneParentProbe { public $child = 'child'; }",
        )
        .expect("child source should be writable");
    let main_path = root.join("main.php");
    let source = "<?php require_once __DIR__ . '/child.php'; $object = new CloneChildProbe(); $copy = clone $object; echo $copy->parent, '|', $copy->child;";
    std::fs::write(&main_path, source).expect("main source should be writable");
    let loader = IncludeLoader::for_root(root.clone()).expect("include loader");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(loader),
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );

    let _ = std::fs::remove_file(parent_path);
    let _ = std::fs::remove_file(child_path);
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_dir(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"parent|child");
}

#[test]
fn included_child_can_call_protected_parent_method_case_insensitively() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-included-protected-parent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let parent_path = root.join("parent.php");
    std::fs::write(
            &parent_path,
            "<?php class RestControllerBase { protected function add_additional_fields_schema($schema) { return $schema . '|parent'; } }",
        )
        .expect("parent source should be writable");
    let child_path = root.join("child.php");
    std::fs::write(
            &child_path,
            "<?php require_once __DIR__ . '/parent.php'; class RestPostsController extends RestControllerBase { public function get_item_schema() { return $this->add_additional_fields_schema('schema'); } }",
        )
        .expect("child source should be writable");
    let main_path = root.join("main.php");
    let source = "<?php require_once __DIR__ . '/child.php'; $controller = new restpostscontroller(); echo $controller->get_item_schema();";
    std::fs::write(&main_path, source).expect("main source should be writable");
    let loader = IncludeLoader::for_root(root.clone()).expect("include loader");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(loader),
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );

    let _ = std::fs::remove_file(parent_path);
    let _ = std::fs::remove_file(child_path);
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_dir(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"schema|parent");
}

#[test]
fn included_child_protected_parent_call_stays_valid_after_method_cache_warms() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-included-protected-parent-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let parent_path = root.join("parent.php");
    std::fs::write(
            &parent_path,
            "<?php class RestControllerBase { protected function add_additional_fields_schema($schema) { return $schema . '|parent'; } }",
        )
        .expect("parent source should be writable");
    let child_path = root.join("child.php");
    std::fs::write(
            &child_path,
            "<?php require_once __DIR__ . '/parent.php'; class RestPostsController extends RestControllerBase { public function get_item_schema($schema) { return $this->add_additional_fields_schema($schema); } }",
        )
        .expect("child source should be writable");
    let main_path = root.join("main.php");
    let source = "<?php
            require_once __DIR__ . '/child.php';
            $controller = new restpostscontroller();
            for ($i = 0; $i < 16; $i++) {
                echo $controller->get_item_schema('schema'), ';';
            }
        ";
    std::fs::write(&main_path, source).expect("main source should be writable");
    let loader = IncludeLoader::for_root(root.clone()).expect("include loader");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(loader),
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );

    let _ = std::fs::remove_file(parent_path);
    let _ = std::fs::remove_file(child_path);
    let _ = std::fs::remove_file(main_path);
    let _ = std::fs::remove_dir(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "schema|parent;".repeat(16));
}

#[test]
fn runtime_layout_counters_preserve_cow_reference_and_destructor_order() {
    let source = "<?php
            class RuntimeLayoutD {
                public function __destruct() { echo '|destruct'; }
            }
            $a = [1, 2];
            $b = $a;
            $b[] = 3;
            echo count($a), ':', count($b);
            $x = 10;
            $y =& $x;
            $y = 12;
            echo '|', $x;
            $o = new RuntimeLayoutD();
            echo '|before';
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2:3|12|before|destruct");
    let counters = result.counters.expect("counters");
    assert!(counters.value_clones > 0, "{counters:?}");
    assert!(counters.string_allocations > 0, "{counters:?}");
    assert!(counters.array_handle_clones > 0, "{counters:?}");
    assert!(counters.cow_separations > 0, "{counters:?}");
    assert!(counters.reference_cell_creations > 0, "{counters:?}");
    assert!(counters.object_allocations > 0, "{counters:?}");
}

#[test]
fn destructors_run_when_local_last_reference_is_replaced_or_unset() {
    let replaced = execute_source(
        "<?php class D { public $name; function __construct($name) { $this->name = $name; } function __destruct() { echo 'd:', $this->name, '|'; } } $a = new D('a'); $a = null; echo 'body|';",
    );

    assert!(replaced.status.is_success(), "{:?}", replaced.status);
    assert_eq!(replaced.output.as_bytes(), b"d:a|body|");

    let retained_alias = execute_source(
        "<?php class D { public $name; function __construct($name) { $this->name = $name; } function __destruct() { echo 'd:', $this->name, '|'; } } $a = new D('a'); $b = $a; $a = null; echo 'body|';",
    );

    assert!(
        retained_alias.status.is_success(),
        "{:?}",
        retained_alias.status
    );
    assert_eq!(retained_alias.output.as_bytes(), b"body|d:a|");

    let unset = execute_source(
        "<?php class D { function __destruct() { echo 'unset|'; } } $a = new D(); unset($a); echo 'body|';",
    );

    assert!(unset.status.is_success(), "{:?}", unset.status);
    assert_eq!(unset.output.as_bytes(), b"unset|body|");
}

#[test]
fn destructor_teardown_preserves_nested_returned_objects() {
    let result = execute_source(
        "<?php class D { function __destruct() { echo 'd|'; } } function make() { $o = new D(); $local = [$o]; return ['keep' => $local]; } $keep = make(); echo 'after|'; unset($keep); echo 'done|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"after|d|done|");
}

#[test]
fn destructor_exceptions_from_local_release_are_catchable() {
    let result = execute_source(
        "<?php class D { function __destruct() { throw new Exception('boom'); } } try { $a = new D(); $a = null; echo 'after|'; } catch (Exception $e) { echo 'caught:', $e->getMessage(), '|'; } echo 'done|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"caught:boom|done|");
}

#[test]
fn inaccessible_destructor_from_local_release_is_catchable_error() {
    let result = execute_source(
        "<?php class D { private function __destruct() {} } try { $a = new D(); unset($a); echo 'after|'; } catch (Error $e) { echo 'caught:', $e->getMessage(), '|'; } echo 'done|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"caught:Call to private D::__destruct() from global scope|done|"
    );
}

#[test]
fn inaccessible_destructor_at_shutdown_emits_warning_and_is_ignored() {
    let result = execute_source(
        "<?php class Base { private function __destruct() { echo 'bad|'; } } class Derived extends Base {} $a = new Derived(); echo 'body|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.starts_with("body|"), "{output}");
    assert!(
            output.contains(
                "Warning: Call to private Derived::__destruct() from global scope during shutdown ignored in Unknown on line 0"
            ),
            "{output}"
        );
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_VM_DESTRUCTOR_VISIBILITY_WARNING"
    );
}

#[test]
fn static_property_release_runs_destructor_in_current_class_scope() {
    let result = execute_source(
        "<?php class D { public static $slot; public static $count = 0; function __construct() { self::$count++; } static function destroy() { self::$slot = null; } protected function __destruct() { self::$count--; echo 'd|'; } } D::$slot = new D(); D::destroy(); echo D::$count;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"d|0");
}

#[test]
fn destructors_register_clones_and_reentrant_objects() {
    let result = execute_source(
        "<?php class D { public $name; function __construct($name) { $this->name = $name; } function __clone() { $this->name = 'clone'; } function __destruct() { echo 'd:', $this->name, '|'; if ($this->name === 'clone') { new D('late'); } } } $a = new D('a'); $b = clone $a; echo 'body|';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"body|d:clone|d:late|d:a|");
}

#[test]
fn destructor_throw_becomes_shutdown_runtime_error() {
    let result = execute_source(
        "<?php class D { function __destruct() { echo 'destruct|'; throw new Exception('boom'); } } new D(); echo 'body|';",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_uncaught_exception_output_prefix(
        &result.output.to_string_lossy(),
        "destruct|",
        "Exception",
        "boom",
    );
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
}

#[test]
fn gc_snapshot_tracks_vm_roots_and_cycle_candidates() {
    let class = RuntimeClassEntry {
        name: "GcBox".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    };
    let object = ObjectRef::new(&class);
    object.set_property("self", Value::Object(object.clone()));

    let mut frame = Frame::new(FunctionId::new(0), 1, 1);
    frame
        .registers
        .set(RegId::new(0), Value::Object(object.clone()))
        .expect("register");
    frame
        .locals
        .set(LocalId::new(0), Value::Object(object.clone()))
        .expect("local");
    let mut stack = CallStack::new();
    stack.push(frame);

    let mut state = ExecutionState::default();
    state.static_locals.insert(
        (0, "cached".to_owned()),
        ReferenceCell::new(Value::Object(object.clone())),
    );
    state.static_properties.insert(
        ("GcBox".to_owned(), "slot".to_owned()),
        Value::Object(object.clone()),
    );
    state
        .enum_cases
        .insert(("GcEnum".to_owned(), "A".to_owned()), object.clone());
    state.destructor_queue.register(
        object.clone(),
        "GcBox".to_owned(),
        FunctionId::new(0),
        None,
        DestructorVisibility::Public,
    );

    let snapshot = gc_snapshot_from_vm_roots(&stack, &state);
    let object_id = GcEntityId::new(GcEntityKind::Object, object.id());

    assert!(snapshot.contains(object_id));
    let node = &snapshot.nodes[&object_id];
    assert!(node.roots.contains(&"frame0.r0".to_owned()));
    assert!(node.roots.contains(&"frame0.local0".to_owned()));
    assert!(
        node.roots
            .contains(&"static-property:GcBox::slot".to_owned())
    );
    assert!(node.roots.contains(&"enum-case:GcEnum::A".to_owned()));
    assert!(node.roots.contains(&"destructor-queue:0".to_owned()));
    assert!(
        snapshot
            .cycle_candidates
            .iter()
            .any(|candidate| candidate.root == object_id)
    );
}

#[test]
fn clone_with_applies_public_property_replacements_to_copy() {
    let result = execute_source(
        "<?php class Box { public $name; public $count; } $original = new Box(); $original->name = 'old'; $original->count = 1; $copy = clone($original, ['name' => 'new', 'count' => 2]); echo $original->name, ':', $original->count, '|', $copy->name, ':', $copy->count;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"old:1|new:2");
}

#[test]
fn clone_with_classifies_unsupported_properties() {
    let readonly = execute_source(
        "<?php class Locked { public readonly $value; } $original = new Locked(); $copy = clone($original, ['value' => 1]);",
    );
    assert_eq!(readonly.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(readonly.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
            readonly
                .output
                .to_string_lossy()
                .contains("Uncaught Error: Cannot modify protected(set) readonly property Locked::$value from global scope"),
            "{}",
            readonly.output.to_string_lossy()
        );
}

#[test]
fn var_dump_uses_debug_info_with_original_object_handle() {
    let result = execute_source(
        r#"<?php
class DebugInfoBox {
    public $hidden = 99;
    public function __debugInfo(): array {
        return ["visible" => 1, 0 => "zero"];
    }
}
$box = new DebugInfoBox();
echo spl_object_id($box), "\n";
var_dump($box);
"#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    let (object_id, dump) = output
        .split_once('\n')
        .expect("spl_object_id line should precede var_dump output");
    assert_eq!(
        dump,
        format!(
            "object(DebugInfoBox)#{object_id} (2) {{\n  [\"visible\"]=>\n  int(1)\n  [0]=>\n  string(4) \"zero\"\n}}\n"
        )
    );
}

#[test]
fn methods_execute_instance_calls_and_this_property() {
    let result = execute_source(
        "<?php class Box { public $value; function __construct($value) { $this->value = $value; } function get() { return $this->value; } function plus($value) { return $this->get() + $value; } } $box = new Box(7); echo $box->get(), '|', $box->plus(5);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7|12");
}

#[test]
fn methods_execute_static_calls() {
    let result = execute_source(
        "<?php class Util { static function name() { return 'ok'; } } echo Util::name();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn scoped_missing_static_calls_use_inherited_instance_magic_call() {
    let result = execute_source(
        "<?php class Base { public function __call($name, $args) { echo $name, '|'; } } class Child extends Base { public function run() { static::first(); parent::second(); } } (new Child())->run();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"first|second|");
}

#[test]
fn methods_execute_inheritance_visibility_and_static_scope() {
    let inherited = execute_source(
        "<?php class Base { public $value; function set($value) { $this->value = $value; } function get() { return $this->value; } } class Child extends Base { function label() { return 'child'; } } $child = new Child(); $child->set(9); echo $child->get(), '|', $child->label();",
    );
    assert!(inherited.status.is_success(), "{:?}", inherited.status);
    assert_eq!(inherited.output.as_bytes(), b"9|child");

    let private_scope = execute_source(
        "<?php class A { private function x() { return 'A'; } public function call() { return $this->x(); } } class B extends A { private function x() { return 'B'; } public function own() { return $this->x(); } } $b = new B(); echo $b->call(), '|', $b->own();",
    );
    assert!(
        private_scope.status.is_success(),
        "{:?}",
        private_scope.status
    );
    assert_eq!(private_scope.output.as_bytes(), b"A|B");

    let protected_scope = execute_source(
        "<?php class A { protected function x() { return 'A'; } } class B extends A { public function call() { return $this->x(); } } echo (new B())->call();",
    );
    assert!(
        protected_scope.status.is_success(),
        "{:?}",
        protected_scope.status
    );
    assert_eq!(protected_scope.output.as_bytes(), b"A");

    let protected_child_override = execute_source(
        "<?php class ProtectedMethodParent { protected function value() { return 'parent'; } public function call() { return $this->value(); } } class ProtectedMethodChild extends ProtectedMethodParent { protected function value() { return 'child'; } } echo (new ProtectedMethodChild())->call();",
    );
    assert!(
        protected_child_override.status.is_success(),
        "{:?}",
        protected_child_override.status
    );
    assert_eq!(protected_child_override.output.as_bytes(), b"child");

    let scoped_non_static_call = execute_source(
        "<?php class ScopedCallBase { public function value() { return $this->label; } } class ScopedCallChild extends ScopedCallBase { public $label = 'child'; public function callParent() { return parent::value(); } } echo (new ScopedCallChild())->callParent();",
    );
    assert!(
        scoped_non_static_call.status.is_success(),
        "{:?}",
        scoped_non_static_call.status
    );
    assert_eq!(scoped_non_static_call.output.as_bytes(), b"child");

    let static_scope = execute_source(
        "<?php class A { static function name() { return 'A'; } } class B extends A { static function own() { return self::name() . parent::name(); } } echo B::own();",
    );
    assert!(
        static_scope.status.is_success(),
        "{:?}",
        static_scope.status
    );
    assert_eq!(static_scope.output.as_bytes(), b"AA");
}

#[test]
fn methods_reject_incompatible_parent_overrides() {
    let lowered_visibility = execute_source(
        "<?php class Base { public function show() {} } class Child extends Base { protected function show() {} }",
    );
    assert_eq!(
        lowered_visibility.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        lowered_visibility
            .status
            .message()
            .expect("compile message")
            .contains("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE"),
        "{:?}",
        lowered_visibility.status
    );
    assert!(matches!(
        first_vm_compile_payload(&lowered_visibility),
        VmCompileDiagnostic::MethodVisibilityOverride {
            class_name,
            method_name,
            ..
        } if class_name == "child" && method_name == "show"
    ));

    let static_to_instance = execute_source(
        "<?php class Base { public static function show() {} } class Child extends Base { public function show() {} }",
    );
    assert_eq!(
        static_to_instance.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        static_to_instance
            .status
            .message()
            .expect("compile message")
            .contains("Cannot make static method base::show() non static in class child"),
        "{:?}",
        static_to_instance.status
    );
    assert!(matches!(
        first_vm_compile_payload(&static_to_instance),
        VmCompileDiagnostic::StaticMethodOverride {
            class_name,
            method_name,
            parent_is_static: true,
            ..
        } if class_name == "child" && method_name == "show"
    ));

    let instance_to_static = execute_source(
        "<?php class Base { public function show() {} } class Child extends Base { public static function show() {} }",
    );
    assert_eq!(
        instance_to_static.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        instance_to_static
            .status
            .message()
            .expect("compile message")
            .contains("Cannot make non static method base::show() static in class child"),
        "{:?}",
        instance_to_static.status
    );

    let narrowed_parameter_type = execute_source(
        "<?php class Base { public function accept($value) {} } class Child extends Base { public function accept(array $value) {} }",
    );
    assert_eq!(
        narrowed_parameter_type.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            narrowed_parameter_type
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: Declaration of Child::accept(array $value) must be compatible with Base::accept($value)"
                ),
            "{:?}",
            narrowed_parameter_type.status
        );
    assert!(matches!(
        first_vm_compile_payload(&narrowed_parameter_type),
        VmCompileDiagnostic::MethodSignatureOverride {
            class_name,
            method_name,
            ..
        } if class_name == "Child" && method_name == "accept"
    ));

    let class_type_name_preserves_source_case = execute_source(
        "<?php class SomeClass {} class Base { public function accept(SomeClass $value) {} } class Child extends Base { public function accept(array $value) {} }",
    );
    assert_eq!(
        class_type_name_preserves_source_case.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            class_type_name_preserves_source_case
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: Declaration of Child::accept(array $value) must be compatible with Base::accept(SomeClass $value)"
                ),
            "{:?}",
            class_type_name_preserves_source_case.status
        );

    let removed_optional_parameter = execute_source(
        "<?php class Base { public function accept($value = 1) {} } class Child extends Base { public function accept() {} }",
    );
    assert_eq!(
        removed_optional_parameter.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            removed_optional_parameter
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: Declaration of Child::accept() must be compatible with Base::accept($value = 1)"
                ),
            "{:?}",
            removed_optional_parameter.status
        );
}

#[test]
fn properties_reject_incompatible_parent_redeclarations() {
    let lowered_visibility =
        execute_source("<?php class Base { public $p; } class Child extends Base { private $p; }");
    assert_eq!(
        lowered_visibility.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            lowered_visibility
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: Access level to Child::$p must be public (as in class Base)"
                ),
            "{:?}",
            lowered_visibility.status
        );
    assert!(matches!(
        first_vm_compile_payload(&lowered_visibility),
        VmCompileDiagnostic::PropertyVisibilityOverride {
            class_name,
            property_name,
            ..
        } if class_name == "Child" && property_name == "p"
    ));

    let static_to_instance = execute_source(
        "<?php class Base { public static $p; } class Child extends Base { public $p; }",
    );
    assert_eq!(
        static_to_instance.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        static_to_instance
            .status
            .message()
            .expect("compile message")
            .contains("Cannot redeclare static Base::$p as non static Child::$p"),
        "{:?}",
        static_to_instance.status
    );
    assert!(matches!(
        first_vm_compile_payload(&static_to_instance),
        VmCompileDiagnostic::PropertyStaticOverride {
            class_name,
            property_name,
            parent_is_static: true,
            ..
        } if class_name == "Child" && property_name == "p"
    ));

    let instance_to_static = execute_source(
        "<?php class Base { public $p; } class Child extends Base { public static $p; }",
    );
    assert_eq!(
        instance_to_static.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        instance_to_static
            .status
            .message()
            .expect("compile message")
            .contains("Cannot redeclare non static Base::$p as static Child::$p"),
        "{:?}",
        instance_to_static.status
    );

    let private_parent = execute_source(
        "<?php class Base { private $p = 'base'; } class Child extends Base { public static $p = 'child'; } echo Child::$p;",
    );
    assert!(
        private_parent.status.is_success(),
        "{:?}",
        private_parent.status
    );
    assert_eq!(private_parent.output.as_bytes(), b"child");
}

#[test]
fn static_property_isset_empty_execute_without_fetching_missing_property() {
    let result = execute_source(
        "<?php class C { public static $q = 0; public static $r = null; public static $s = 1; } echo isset(C::$p) ? 'yes' : 'no', '|', empty(C::$p) ? 'empty' : 'filled', '|', isset(C::$q) ? 'yes' : 'no', '|', empty(C::$q) ? 'empty' : 'filled', '|', isset(C::$r) ? 'yes' : 'no', '|', empty(C::$r) ? 'empty' : 'filled', '|', isset(C::$s) ? 'yes' : 'no', '|', empty(C::$s) ? 'empty' : 'filled';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"no|empty|yes|empty|no|empty|yes|filled"
    );
}

#[test]
fn dynamic_class_static_property_fetch_and_assign_execute() {
    let result = execute_source(
        "<?php class Mailer { public static $validator = 'old'; } $phpmailer = new Mailer(); $phpmailer::$validator = 'new'; echo Mailer::$validator, '|', $phpmailer::$validator;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"new|new");
}

#[test]
fn static_property_dimension_isset_and_unset_execute() {
    let result = execute_source(
        "<?php
            class C {
                public static $map = ['id' => 'ID'];
                static function run($key) {
                    var_dump(isset(self::$map[$key]));
                    unset(self::$map[$key]);
                    var_dump(isset(self::$map[$key]));
                }
            }
            C::run('id');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "bool(true)\nbool(false)\n");
}

#[test]
fn constants_reject_incompatible_parent_redeclarations() {
    let public_to_protected = execute_source(
        "<?php class Base { public const TOKEN = 1; } class Child extends Base { protected const TOKEN = 2; }",
    );
    assert_eq!(
        public_to_protected.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            public_to_protected
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: Access level to Child::TOKEN must be public (as in class Base)"
                ),
            "{:?}",
            public_to_protected.status
        );
    assert!(matches!(
        first_vm_compile_payload(&public_to_protected),
        VmCompileDiagnostic::ClassConstantVisibilityOverride {
            class_name,
            constant_name,
            ..
        } if class_name == "Child" && constant_name == "TOKEN"
    ));

    let protected_to_private = execute_source(
        "<?php class Base { protected const TOKEN = 1; } class Child extends Base { private const TOKEN = 2; }",
    );
    assert_eq!(
        protected_to_private.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            protected_to_private
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: Access level to Child::TOKEN must be protected (as in class Base) or weaker"
                ),
            "{:?}",
            protected_to_private.status
        );

    let private_parent = execute_source(
        "<?php class Base { private const TOKEN = 'base'; } class Child extends Base { public const TOKEN = 'child'; } echo Child::TOKEN;",
    );
    assert!(
        private_parent.status.is_success(),
        "{:?}",
        private_parent.status
    );
    assert_eq!(private_parent.output.as_bytes(), b"child");
}

#[test]
fn constants_do_not_inherit_private_parent_constants() {
    let result = execute_source(
        "<?php class Base { private const TOKEN = 'base'; } class Child extends Base { public static function check() { try { var_dump(self::TOKEN); } catch (Error $e) { echo $e->getMessage(); } } } Child::check();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"Undefined constant Child::TOKEN");
}

#[test]
fn concrete_classes_validate_inherited_interface_contracts() {
    let abstract_defers_missing_method = execute_source(
        "<?php interface Contract { public function build(); } abstract class Base implements Contract {}",
    );
    assert!(
        abstract_defers_missing_method.status.is_success(),
        "{:?}",
        abstract_defers_missing_method.status
    );

    let inherited_signature_mismatch = execute_source(
        "<?php interface Contract { public function __construct(); } abstract class Base implements Contract {} class Child extends Base { public function __construct($value) {} }",
    );
    assert_eq!(
        inherited_signature_mismatch.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            inherited_signature_mismatch
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_INTERFACE_METHOD_SIGNATURE: Declaration of Child::__construct($value) must be compatible with Contract::__construct()"
                ),
            "{:?}",
            inherited_signature_mismatch.status
        );

    let concrete_parent_signature_mismatch = execute_source(
        "<?php interface Contract { public function __construct(); } class Base implements Contract { public function __construct() {} } class Child extends Base { public function __construct($value) {} }",
    );
    assert_eq!(
        concrete_parent_signature_mismatch.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
            concrete_parent_signature_mismatch
                .status
                .message()
                .expect("compile message")
                .contains(
                    "E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: Declaration of Child::__construct($value) must be compatible with Contract::__construct()"
                ),
            "{:?}",
            concrete_parent_signature_mismatch.status
        );
}

#[test]
fn interfaces_reject_private_methods_bodies_and_plain_properties() {
    let protected_constant = execute_source("<?php interface I { protected const TOKEN = 1; }");
    assert_eq!(
        protected_constant.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        protected_constant
            .status
            .message()
            .expect("compile message")
            .contains("Access type for interface constant I::TOKEN must be public"),
        "{:?}",
        protected_constant.status
    );
    assert!(matches!(
        first_vm_compile_payload(&protected_constant),
        VmCompileDiagnostic::InterfaceConstantVisibility {
            class_name,
            constant_name,
        } if class_name == "I" && constant_name == "TOKEN"
    ));

    let private_method = execute_source("<?php interface I { private function err(); }");
    assert_eq!(
        private_method.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        private_method
            .status
            .message()
            .expect("compile message")
            .contains("Access type for interface method I::err() must be public"),
        "{:?}",
        private_method.status
    );
    assert!(matches!(
        first_vm_compile_payload(&private_method),
        VmCompileDiagnostic::InterfaceMethodVisibility {
            class_name,
            method_name,
        } if class_name == "I" && method_name == "err"
    ));

    let method_body = execute_source("<?php interface I { function err() {} }");
    assert_eq!(method_body.status.exit_status(), ExitStatus::CompileError);
    assert!(
        method_body
            .status
            .message()
            .expect("compile message")
            .contains("Interface function I::err() cannot contain body"),
        "{:?}",
        method_body.status
    );
    assert!(matches!(
        first_vm_compile_payload(&method_body),
        VmCompileDiagnostic::InterfaceMethodBody {
            class_name,
            method_name,
        } if class_name == "I" && method_name == "err"
    ));

    let plain_property = execute_source("<?php interface I { public $member; }");
    assert_eq!(
        plain_property.status.exit_status(),
        ExitStatus::CompileError
    );
    assert!(
        plain_property
            .status
            .message()
            .expect("compile message")
            .contains("Interfaces may only include hooked properties"),
        "{:?}",
        plain_property.status
    );
    assert!(matches!(
        first_vm_compile_payload(&plain_property),
        VmCompileDiagnostic::InterfaceProperty {
            class_name,
            property_name,
        } if class_name == "I" && property_name == "member"
    ));
}

#[test]
fn class_table_compile_errors_carry_typed_payloads() {
    let final_class = execute_source("<?php final class Base {} class Child extends Base {}");
    assert!(matches!(
        first_vm_compile_payload(&final_class),
        VmCompileDiagnostic::FinalClassExtend {
            class_name,
            parent_class_name,
        } if class_name == "child" && parent_class_name == "base"
    ));

    let final_method = execute_source(
        "<?php class Base { final public function seal() {} } class Child extends Base { public function seal() {} }",
    );
    assert!(matches!(
        first_vm_compile_payload(&final_method),
        VmCompileDiagnostic::FinalMethodOverride {
            class_name,
            method_name,
            parent_class_name,
        } if class_name == "Child" && method_name == "seal" && parent_class_name == "Base"
    ));

    let extends_interface = execute_source("<?php interface I {} class Child extends I {}");
    assert!(matches!(
        first_vm_compile_payload(&extends_interface),
        VmCompileDiagnostic::ClassExtendsInterface {
            class_name,
            interface_name,
        } if class_name == "Child" && interface_name == "I"
    ));

    let implements_non_interface =
        execute_source("<?php class Base {} class Child implements Base {}");
    assert!(matches!(
        first_vm_compile_payload(&implements_non_interface),
        VmCompileDiagnostic::ImplementsNonInterface {
            class_name,
            target_name,
            ..
        } if class_name == "Child" && target_name == "Base"
    ));

    let traversable = execute_source("<?php class DirectTraversable implements Traversable {}");
    assert!(matches!(
        first_vm_compile_payload(&traversable),
        VmCompileDiagnostic::TraversableDirectImplementation { class_name }
            if class_name == "DirectTraversable"
    ));
}

#[test]
fn interface_instantiation_raises_catchable_error() {
    let result = execute_source(
        "<?php interface I {} try { new I(); } catch (Error $e) { echo $e->getMessage(); }",
    );
    assert_eq!(result.status.exit_status(), ExitStatus::Success);
    assert_eq!(result.output.as_bytes(), b"Cannot instantiate interface i");
}

#[test]
fn methods_execute_private_and_protected_property_scope() {
    let private_scope = execute_source(
        "<?php class A { private $x; public function setA($x) { $this->x = $x; } public function getA() { return $this->x; } } class B extends A { private $x; public function setB($x) { $this->x = $x; } public function getB() { return $this->x; } } $b = new B(); $b->setA('A'); $b->setB('B'); echo $b->getA(), '|', $b->getB();",
    );
    assert!(
        private_scope.status.is_success(),
        "{:?}",
        private_scope.status
    );
    assert_eq!(private_scope.output.as_bytes(), b"A|B");

    let private_parent_public_child = execute_source(
        "<?php class PrivateParentProperty { private $p = 'A'; public function showA() { echo $this->p; } } class PublicChildProperty extends PrivateParentProperty { public $p = 'B'; public function showB() { echo $this->p; } } $object = new PublicChildProperty(); $object->showA(); $object->showB(); echo $object->p;",
    );
    assert!(
        private_parent_public_child.status.is_success(),
        "{:?}",
        private_parent_public_child.status
    );
    assert_eq!(private_parent_public_child.output.as_bytes(), b"ABB");

    let protected_child_override = execute_source(
        "<?php class ProtectedParentProperty { protected $p = 'A'; public function showA() { echo $this->p; } } class ProtectedChildProperty extends ProtectedParentProperty { protected $p = 'B'; public function showB() { echo $this->p; } } $object = new ProtectedChildProperty(); $object->showA(); $object->showB();",
    );
    assert!(
        protected_child_override.status.is_success(),
        "{:?}",
        protected_child_override.status
    );
    assert_eq!(protected_child_override.output.as_bytes(), b"BB");

    let protected_scope = execute_source(
        "<?php class A { protected $x; public function setA($x) { $this->x = $x; } } class B extends A { public function read() { return $this->x; } } $b = new B(); $b->setA('ok'); echo $b->read();",
    );
    assert!(
        protected_scope.status.is_success(),
        "{:?}",
        protected_scope.status
    );
    assert_eq!(protected_scope.output.as_bytes(), b"ok");

    let private_debug_labels = execute_source(
        "<?php class A { private $c; } class B extends A { private $c; } class C extends B { private $c; } var_dump(new C());",
    );
    assert!(
        private_debug_labels.status.is_success(),
        "{:?}",
        private_debug_labels.status
    );
    let output = private_debug_labels.output.to_string_lossy();
    assert!(output.contains("[\"c\":\"A\":private]"), "{output}");
    assert!(output.contains("[\"c\":\"B\":private]"), "{output}");
    assert!(output.contains("[\"c\":\"C\":private]"), "{output}");

    let recreate_parent_private_as_dynamic = execute_source(
        "<?php #[AllowDynamicProperties] class C { private $p = 'test'; function unsetPrivate() { unset($this->p); } } class D extends C { function setP() { $this->p = 'changed in D'; } } $d = new D(); $d->unsetPrivate(); $d->setP(); echo $d->p, '|'; $d = new D(); $d->unsetPrivate(); $d->p = 'changed globally'; echo $d->p;",
    );
    assert!(
        recreate_parent_private_as_dynamic.status.is_success(),
        "{:?}",
        recreate_parent_private_as_dynamic.status
    );
    assert_eq!(
        recreate_parent_private_as_dynamic.output.as_bytes(),
        b"changed in D|changed globally"
    );
}

#[test]
fn serialize_invokes_sleep_and_warns_for_missing_property() {
    let result = execute_source(
        r#"<?php
class foo {
    private $private = 'private';
    protected $protected = 'protected';
    public $public = 'public';
    public function __sleep() {
        return array('private', 'protected', 'public', 'no_such');
    }
}
$foo = new foo();
$data = serialize($foo);
var_dump(str_replace("\0", '\0', $data));
"#,
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
            output.contains(
                "Warning: serialize(): \"no_such\" returned as member variable from __sleep() but does not exist"
            ),
            "{output}"
        );
    assert!(
            output.contains(
                r#"string(114) "O:3:"foo":3:{s:12:"\0foo\0private";s:7:"private";s:12:"\0*\0protected";s:9:"protected";s:6:"public";s:6:"public";}""#
            ),
            "{output}"
        );

    let mangled_parent_private = execute_source(
        r#"<?php
class foo {
    private $private = 'private';
    protected $protected = 'protected';
    public $public = 'public';
}
class bar extends foo {
    public function __sleep() {
        return array("\0foo\0private", 'protected', 'public');
    }
}
var_dump(str_replace("\0", '\0', serialize(new bar())));
"#,
    );
    assert!(
        mangled_parent_private.status.is_success(),
        "{:?}",
        mangled_parent_private.status
    );
    let output = mangled_parent_private.output.to_string_lossy();
    assert!(!output.contains("Warning: serialize()"), "{output}");
    assert!(
            output.contains(
                r#"string(114) "O:3:"bar":3:{s:12:"\0foo\0private";s:7:"private";s:12:"\0*\0protected";s:9:"protected";s:6:"public";s:6:"public";}""#
            ),
            "{output}"
        );
}

#[test]
fn unserialize_autoloads_missing_class_as_incomplete_class() {
    let result = execute_source(
        r#"<?php
spl_autoload_register(function ($name) {
    echo "in autoload: $name\n";
});

var_dump(unserialize('O:1:"C":0:{}'));
"#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("in autoload: C\n"), "{output}");
    assert!(
        output.contains("object(__PHP_Incomplete_Class)#"),
        "{output}"
    );
    assert!(
        output.contains("[\"__PHP_Incomplete_Class_Name\"]=>\n  string(1) \"C\""),
        "{output}"
    );
}

#[test]
fn isset_empty_property_dimensions_execute_in_class_scope() {
    let result = execute_source(
        "<?php class C { private $a = ['x' => [1], 'empty' => []]; public function run($k) { echo isset($this->a[$k]) ? 'yes' : 'no'; echo '|'; echo empty($this->a['empty']) ? 'empty' : 'filled'; } } (new C())->run('x');",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"yes|empty");

    let missing = execute_source(
        "<?php class C { private $a = ['x' => [1]]; public function run($k) { echo isset($this->a[$k]) ? 'yes' : 'no'; } } (new C())->run('missing');",
    );
    assert!(missing.status.is_success(), "{:?}", missing.status);
    assert_eq!(missing.output.as_bytes(), b"no");
}

#[test]
fn empty_method_call_executes_as_value_emptiness() {
    let result = execute_source(
        "<?php class C { public function get($key) { return $key === 'zero' ? '0' : 'value'; } } $c = new C(); echo empty($c->get('zero')) ? 'empty' : 'filled'; echo '|'; echo empty($c->get('name')) ? 'empty' : 'filled';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"empty|filled");
}

#[test]
fn property_dimensions_assignment_append_and_unset_execute_in_class_scope() {
    let result = execute_source(
        "<?php class C { private $items = []; public function run() { $this->items['a']['b'] = 3; $this->items[] = 'tail'; echo $this->items['a']['b'], '|', $this->items[0]; unset($this->items['a']['b']); echo '|', isset($this->items['a']['b']) ? 'bad' : 'gone'; } } (new C())->run();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3|tail|gone");
}

#[test]
fn property_dimensions_execute_compound_assignment_through_binary_ops() {
    let result = execute_source(
        "<?php class C { private $cache = []; public function run() { $group = 'g'; $key = 'k'; $this->cache[$group][$key] = 1; $this->cache[$group][$key] += 4; $this->cache[$group][$key] -= 2; echo $this->cache[$group][$key]; } } (new C())->run();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3");
}

#[test]
fn property_execute_defaults_readonly_static_dynamic_and_state_ops() {
    let defaults = execute_source(
        "<?php class C { public $name = 'box'; public int $count; } $c = new C(); echo $c->name, '|'; $c->count = 3; echo $c->count;",
    );
    assert!(defaults.status.is_success(), "{:?}", defaults.status);
    assert_eq!(defaults.output.as_bytes(), b"box|3");

    let uninitialized =
        execute_source("<?php class C { public int $count; } echo (new C())->count;");
    assert_eq!(uninitialized.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        uninitialized.diagnostics[0].id(),
        "E_PHP_VM_UNCAUGHT_EXCEPTION"
    );
    assert!(
        uninitialized.output.to_string_lossy().contains(
            "Uncaught Error: Typed property C::$count must not be accessed before initialization"
        ),
        "{}",
        uninitialized.output.to_string_lossy()
    );

    let caught_uninitialized = execute_source(
        "<?php class C { public int $count; } try { echo (new C())->count; } catch (Error $e) { echo 'uninitialized'; }",
    );
    assert!(
        caught_uninitialized.status.is_success(),
        "{:?}",
        caught_uninitialized.status
    );
    assert_eq!(caught_uninitialized.output.as_bytes(), b"uninitialized");

    let nullable = execute_source(
        "<?php class C { public ?int $count = null; } $c = new C(); var_dump($c->count); $c->count = 5; echo $c->count, '|'; $c->count = null; var_dump($c->count);",
    );
    assert!(nullable.status.is_success(), "{:?}", nullable.status);
    assert_eq!(nullable.output.as_bytes(), b"NULL\n5|NULL\n");

    let readonly = execute_source(
        "<?php class C { public readonly int $x; public function set($x) { $this->x = $x; } } $c = new C(); $c->set(1); echo $c->x; $c->set(2);",
    );
    assert_eq!(readonly.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(readonly.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        readonly
            .output
            .to_string_lossy()
            .contains("Uncaught Error: property c::$x is already initialized"),
        "{}",
        readonly.output.to_string_lossy()
    );
    assert!(readonly.output.as_bytes().starts_with(b"1"));

    let static_property = execute_source(
        "<?php class C { public static int $count; public static $name = 'x'; } C::$count = 2; echo C::$count, '|', C::$name;",
    );
    assert!(
        static_property.status.is_success(),
        "{:?}",
        static_property.status
    );
    assert_eq!(static_property.output.as_bytes(), b"2|x");

    let invalid_static_scope = execute_source(
        "<?php class C { public function name() { return 'C'; } } try { C::$missing; } catch (Error $e) { echo 'read|'; } try { C::$missing = 2; } catch (Error $e) { echo 'write|'; } try { C::name(); } catch (Error $e) { echo 'method'; }",
    );
    assert!(
        invalid_static_scope.status.is_success(),
        "{:?}",
        invalid_static_scope.status
    );
    assert_eq!(invalid_static_scope.output.as_bytes(), b"read|write|method");

    let dynamic = execute_source("<?php class C {} $c = new C(); $c->x = 5; echo $c->x;");
    assert!(dynamic.status.is_success(), "{:?}", dynamic.status);
    assert_eq!(dynamic.output.as_bytes(), b"5");
    assert_eq!(
        dynamic.diagnostics[0].id(),
        "E_PHP_VM_DYNAMIC_PROPERTY_DEPRECATED"
    );

    let allowed_dynamic = execute_source(
        "<?php #[AllowDynamicProperties] class A {} class B extends A {} $a = new A(); $a->x = 1; $b = new B(); $b->y = 2; echo $a->x, '|', $b->y;",
    );
    assert!(
        allowed_dynamic.status.is_success(),
        "{:?}",
        allowed_dynamic.status
    );
    assert_eq!(allowed_dynamic.output.as_bytes(), b"1|2");
    assert!(allowed_dynamic.diagnostics.is_empty());

    let state_ops = execute_source(
        "<?php class C { public $x = 0; public $y = null; } $c = new C(); echo isset($c->x), isset($c->y), empty($c->x), empty($c->missing); unset($c->x); echo isset($c->x), empty($c->x);",
    );
    assert!(state_ops.status.is_success(), "{:?}", state_ops.status);
    assert_eq!(state_ops.output.as_bytes(), b"1111");
}

#[test]
fn braced_dynamic_property_isset_executes() {
    let result = execute_source(
        "<?php class C { public $x = 1; public $y = null; } $c = new C(); $name = 'x'; $null = 'y'; $missing = 'z'; echo isset($c->{$name}) ? 'yes' : 'no'; echo '|', isset($c->{$null}) ? 'bad' : 'null'; echo '|', isset($c->{$missing}) ? 'bad' : 'missing';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"yes|null|missing");
}

#[test]
fn isset_empty_property_on_null_receiver_executes() {
    let result = execute_source(
        "<?php $x = null; $name = 'p'; var_dump(isset($x->p), empty($x->p), isset($x->p[0]), empty($x->p[0]), isset($x->$name), empty($x->$name));",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(false)\nbool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\n"
    );
}

#[test]
fn methods_report_visibility_errors() {
    let private = execute_source(
        "<?php class Secret { private function hidden() { return 1; } } (new Secret())->hidden();",
    );
    assert_eq!(private.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        private
            .output
            .to_string_lossy()
            .contains("Uncaught Error: Call to private method Secret::hidden() from global scope"),
        "{}",
        private.output.to_string_lossy()
    );

    let protected = execute_source(
        "<?php class Secret { protected function hidden() { return 1; } } (new Secret())->hidden();",
    );
    assert_eq!(protected.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        protected.output.to_string_lossy().contains(
            "Uncaught Error: Call to protected method Secret::hidden() from global scope"
        ),
        "{}",
        protected.output.to_string_lossy()
    );

    let private_property = execute_source(
        "<?php class Secret { private $hidden; public function __construct() { $this->hidden = 1; } } echo (new Secret())->hidden;",
    );
    assert_eq!(
        private_property.status.exit_status(),
        ExitStatus::RuntimeError
    );
    assert!(
        private_property
            .output
            .to_string_lossy()
            .contains("Uncaught Error: Cannot access private property Secret::$hidden"),
        "{}",
        private_property.output.to_string_lossy()
    );

    let protected_property = execute_source(
        "<?php class Secret { protected $hidden; public function __construct() { $this->hidden = 1; } } echo (new Secret())->hidden;",
    );
    assert_eq!(
        protected_property.status.exit_status(),
        ExitStatus::RuntimeError
    );
    assert!(
        protected_property
            .output
            .to_string_lossy()
            .contains("Uncaught Error: Cannot access protected property Secret::$hidden"),
        "{}",
        protected_property.output.to_string_lossy()
    );

    let caught_property_write = execute_source(
        "<?php class Secret { private $hidden = 1; } $secret = new Secret(); try { $secret->hidden = 2; } catch (Error $e) { echo 'caught:', $e->getMessage(); }",
    );
    assert!(
        caught_property_write.status.is_success(),
        "{:?}",
        caught_property_write.status
    );
    assert_eq!(
        caught_property_write.output.as_bytes(),
        b"caught:Cannot access private property Secret::$hidden"
    );
}

#[test]
fn dense_static_methods_report_visibility_errors_as_uncaught_throwables() {
    let private = execute_source_with_options(
        "<?php class StaticSecret { private static function hidden() { return 1; } } StaticSecret::hidden();",
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );
    assert_eq!(private.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        private.output.to_string_lossy().contains(
            "Uncaught Error: Call to private method StaticSecret::hidden() from global scope"
        ),
        "{}",
        private.output.to_string_lossy()
    );

    let protected = execute_source_with_options(
        "<?php class StaticSecret { protected static function hidden() { return 1; } } StaticSecret::hidden();",
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );
    assert_eq!(protected.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        protected.output.to_string_lossy().contains(
            "Uncaught Error: Call to protected method StaticSecret::hidden() from global scope"
        ),
        "{}",
        protected.output.to_string_lossy()
    );
}

#[test]
fn private_static_array_callables_are_allowed_from_same_scope() {
    let same_scope = execute_source_with_options(
        r#"<?php
            class Sorter {
                private static function compare($a, $b) { return $a <=> $b; }
                public static function run() {
                    $values = array(2, 1);
                    usort($values, array(self::class, 'compare'));
                    echo implode(',', $values);
                }
            }
            Sorter::run();"#,
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );
    assert!(
        same_scope.status.is_success(),
        "{:?}\n{}",
        same_scope.status,
        same_scope.output.to_string_lossy()
    );
    assert_eq!(same_scope.output.as_bytes(), b"1,2");

    let global_scope = execute_source_with_options(
        r#"<?php
            class Sorter {
                private static function compare($a, $b) { return $a <=> $b; }
            }
            $values = array(2, 1);
            usort($values, array(Sorter::class, 'compare'));"#,
        VmOptions {
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );
    assert_eq!(global_scope.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        global_scope
            .status
            .message()
            .unwrap_or_default()
            .contains("Call to private method Sorter::compare() from global scope"),
        "{:?}",
        global_scope.status
    );
}

#[test]
fn static_class_array_callable_preserves_late_static_binding() {
    let result = execute_source(
        r#"<?php
            class BaseStaticCallable {
                public static function register() {
                    call_user_func(array(static::class, 'parse'));
                }
                public static function parse() {
                    echo static::class;
                }
            }
            class ChildStaticCallable extends BaseStaticCallable {}
            ChildStaticCallable::register();"#,
    );

    assert!(
        result.status.is_success(),
        "{:?}\n{}",
        result.status,
        result.output.to_string_lossy()
    );
    assert_eq!(result.output.as_bytes(), b"ChildStaticCallable");
}

#[test]
fn new_static_preserves_late_static_binding() {
    let result = execute_source(
        r#"<?php
            class BaseStaticFactory {
                public $value;
                public function __construct($value) {
                    $this->value = $value;
                }
                public static function make($value) {
                    return new static($value);
                }
            }
            class ChildStaticFactory extends BaseStaticFactory {}
            $object = ChildStaticFactory::make('ok');
            echo get_class($object), ':', $object->value;"#,
    );

    assert!(
        result.status.is_success(),
        "{:?}\n{}",
        result.status,
        result.output.to_string_lossy()
    );
    assert_eq!(result.output.as_bytes(), b"ChildStaticFactory:ok");
}

#[test]
fn methods_classify_visibility_static_and_this_gaps() {
    let this_outside = execute_source("<?php echo $this;");
    assert_eq!(this_outside.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        this_outside
            .output
            .as_bytes()
            .windows(b"Fatal error: Uncaught Error: Using $this when not in object context".len())
            .any(|window| window
                == b"Fatal error: Uncaught Error: Using $this when not in object context"),
        "{}",
        String::from_utf8_lossy(this_outside.output.as_bytes())
    );

    let caught_this_outside = execute_source(
        "<?php try { $this->a = new stdClass; } catch (Error $e) { echo $e->getMessage(); }",
    );
    assert!(
        caught_this_outside.status.is_success(),
        "{:?}",
        caught_this_outside.status
    );
    assert_eq!(
        caught_this_outside.output.as_bytes(),
        b"Using $this when not in object context"
    );
}

#[test]
fn expressions_modulo_coerces_numeric_operands() {
    let result = execute_source("<?php echo 5.5 % 2;");

    assert!(result.status.is_success(), "{:?}", result.status);
    // The fractional operand deprecates before converting, like the
    // reference: modulo is an int-only context.
    let text = String::from_utf8_lossy(result.output.as_bytes()).into_owned();
    assert!(
        text.contains("Deprecated: Implicit conversion from float 5.5 to int loses precision"),
        "{text}"
    );
    assert!(text.ends_with('1'), "{text}");

    let integral = execute_source("<?php echo 6.0 % 4;");
    assert!(integral.status.is_success(), "{:?}", integral.status);
    assert_eq!(integral.output.as_bytes(), b"2");
}

#[test]
fn variables_execute_assignment_and_fetch() {
    let result = execute_source("<?php $a = 1; echo $a;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1");
}

#[test]
fn trace_is_disabled_by_default() {
    let result = execute_source("<?php echo \"trace off\";");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"trace off");
    assert!(result.trace.is_empty());
    assert_eq!(result.counters, None);
}

#[test]
fn counters_are_opt_in_and_cover_perf_families() {
    let result = execute_source_with_options(
        "<?php function f($v) { return $v . 'x'; } class C { public $p = 0; } $a = [1]; $c = new C(); $ok = $c instanceof C; echo f($a[0]), $c->p;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1x0");
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.instructions_executed > 0);
    assert!(counters.function_calls >= 1);
    assert!(counters.array_dim_fetches >= 1);
    assert!(counters.property_fetches >= 1);
    assert!(counters.type_checks >= 1);
    assert!(counters.string_concats >= 1);
    assert_eq!(counters.guard_failures, 0);
    assert_eq!(counters.cache_hits, 0);
    assert_eq!(counters.cache_misses, 0);
    assert!(counters.literal_intern_hits > 0);
    assert!(counters.literal_intern_misses > 0);
}

#[test]
fn literal_pool_counters_report_repeated_literal_hits() {
    let result = execute_source_with_options(
        "<?php $a = 'same'; $b = 'same'; echo $a, $b, 'same';",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"samesamesame");
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.literal_intern_misses >= 1, "{counters:?}");
    assert!(counters.literal_intern_hits >= 2, "{counters:?}");
}

#[test]
fn dense_bytecode_executes_closures_and_callable_calls() {
    // Closure creation and callable-value calls execute on the dense
    // plan; a try/catch helper stays on the rich plan as a local
    // fallback without pushing the whole program off dense execution.
    let source = "<?php \
            function apply_twice($fn, $x) { return $fn($fn($x)); } \
            function catcher($fn) { try { return $fn(0); } catch (RuntimeException $e) { return $e->getMessage(); } } \
            $double = function ($n) { return $n * 2; }; \
            $sum = 0; \
            for ($i = 1; $i <= 4; $i++) { $sum += apply_twice($double, $i); } \
            $boom = function ($n) { throw new RuntimeException('caught'); }; \
            echo $sum, '|', apply_twice('strrev', 'ab'), '|', catcher($boom);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"40|ab|caught");
    let counters = result.counters.expect("counters");
    assert!(counters.dense_callable_call_hits >= 8, "{counters:?}");
    assert!(
        counters.opcodes.get("bytecode_make_closure").copied() >= Some(2),
        "{counters:?}"
    );
    assert_eq!(
        counters.rich_fallback_functions_executed, 1,
        "only the try/catch helper stays rich: {counters:?}"
    );
}

#[test]
fn direct_frames_elide_argument_vectors_unless_observed() {
    // Plain calls elide the per-call argument snapshot; func_get_args
    // bodies keep it and read the full vector including extras. The Dto
    // property is *typed* so the constructor's assignment stays off the
    // retired stencil property-store leaf (which executed the whole body natively
    // with no frame at all) and keeps exercising the direct-constructor-frame
    // path this test asserts; `get` routes through a local for the same
    // reason, staying off the property-load leaf.
    let source = "<?php \
            function plain($a, $b) { return $a + $b; } \
            function observer() { return implode(\",\", func_get_args()); } \
            class Dto { public int $v = 0; public function __construct($v) { $this->v = $v; } \
                        public function get() { $v = $this->v; return $v; } } \
            $sum = 0; \
            for ($i = 0; $i < 5; $i++) { $sum += plain($i, 1); } \
            $dto = new Dto(41); \
            $c = function ($x) { return $x * 2; }; \
            echo $sum, \"|\", observer(7, 8, 9), \"|\", $dto->get(), \"|\", $c(21);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"15|7,8,9|41|42");
    let counters = result.counters.expect("counters");
    assert!(counters.direct_arg_frame_hits >= 5, "{counters:?}");
    assert!(counters.direct_method_frame_hits >= 1, "{counters:?}");
    assert!(counters.direct_closure_frame_hits >= 1, "{counters:?}");
    assert!(counters.direct_constructor_frame_hits >= 1, "{counters:?}");
    assert!(
        counters.argument_vector_allocations_avoided >= 8,
        "{counters:?}"
    );
    assert!(
        counters
            .direct_frame_fallback_by_reason
            .get("argument_vector_observed")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn dense_direct_calls_transfer_caller_sources_without_owned_values() {
    let source = "<?php \
            function add($a, $b) { return $a + $b; } \
            function forty_two() { return 42; } \
            $sum = 0; for ($i = 0; $i < 6; $i++) { $sum += add($i, 1); } \
            echo $sum, '|', forty_two();";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"21|42");
    let counters = result.counters.expect("counters");
    assert!(counters.dense_call_bare_args_hits >= 7, "{counters:?}");
    assert!(
        counters.prepared_arg_vector_allocations_avoided >= 7,
        "{counters:?}"
    );
    assert!(counters.direct_call_source_reads >= 12, "{counters:?}");
    assert_eq!(counters.direct_call_owned_value_buffers, 0, "{counters:?}");
}

#[test]
fn dense_direct_call_trampoline_handles_deep_php_chains() {
    let result = execute_source_with_options(
        "<?php function depth($n) { if ($n === 0) return 0; return 1 + depth($n - 1); } echo depth(5000);",
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            max_steps: 200_000,
            collect_counters: true,
            collect_profile_spans: false,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5000");
    let counters = result.counters.expect("counters");
    assert!(counters.dense_activation_transfers >= 5_000, "{counters:?}");
    assert_eq!(
        counters.nested_vm_results_avoided, counters.dense_activation_transfers,
        "{counters:?}"
    );
    assert_eq!(
        counters.recursive_dense_calls_avoided, counters.dense_activation_transfers,
        "{counters:?}"
    );
}

#[test]
fn compact_direct_call_is_smaller_than_complex_call() {
    assert!(
        std::mem::size_of::<DirectCall<'static>>() < std::mem::size_of::<FunctionCall<'static>>()
    );
    let destination = CallDestination::OuterReturn;
    assert_eq!(destination, CallDestination::OuterReturn);
}

#[test]
fn dense_builtin_intrinsics_run_before_argument_materialization() {
    let source = "<?php \
            $a = ['key' => 7]; $s = 'Hello'; \
            echo strlen($s), '|', count($a), '|', is_string($s), '|', \
                 array_key_exists('key', $a), '|', str_contains($s, 'ell'), '|', \
                 strtolower($s);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5|1|1|1|1|hello");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.internal_function_dispatches, 0, "{counters:?}");
    for intrinsic in [
        "strlen",
        "count",
        "is_string",
        "array_key_exists",
        "str_contains",
        "strtolower",
    ] {
        assert!(
            counters
                .intrinsic_hits
                .get(intrinsic)
                .copied()
                .unwrap_or_default()
                >= 1,
            "missing pre-args hit for {intrinsic}: {counters:?}"
        );
    }
}

#[test]
fn trivial_getters_and_setters_inline_through_slots() {
    let source = "<?php class Row { public $v = 1; private $s = 2; \
            public function getV() { return $this->v; } \
            public function setV($x) { $this->v = $x; return $this; } \
            public function getS() { return $this->s; } } \
            $r = new Row(); $t = 0; $s = 0; \
            for ($i = 0; $i < 6; $i++) { $t += $r->setV($i)->getV(); $s = $r->getS(); } \
            echo $t, \"|\", $s;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"15|2");
    let counters = result.counters.expect("counters");
    assert!(counters.method_inline_candidates >= 2, "{counters:?}");
    assert!(counters.method_inline_hits >= 8, "{counters:?}");
    // The private-property getter must fall back to generic dispatch.
    assert!(
        counters
            .method_inline_fallback_by_reason
            .get("slot_missing")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn property_ic_hits_use_declared_slots() {
    let source = "<?php class P { public $a = 0; public $b = \"\"; } \
                      function fill($p, $i) { $p->a = $i; $p->b = \"v$i\"; return $p->a; } \
                      $p = new P(); $t = 0; \
                      for ($i = 0; $i < 10; $i++) { $t += fill($p, $i); } \
                      echo $t, $p->b;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"45v9");
    let counters = result.counters.expect("counters");
    assert!(counters.property_ic_hits >= 5, "{counters:?}");
    assert!(counters.property_assign_ic_hits >= 5, "{counters:?}");
    assert!(counters.object_declared_slot_reads >= 5, "{counters:?}");
    assert!(counters.object_declared_slot_writes >= 5, "{counters:?}");
    assert_eq!(
        counters.object_dynamic_property_map_writes, 0,
        "{counters:?}"
    );
}

#[test]
fn dense_dispatch_symbolizes_call_and_array_key_names() {
    let source = "<?php function sym_target($v) { return $v + 1; } \
                      $map = [\"alpha\" => 5]; $total = 0; \
                      for ($i = 0; $i < 6; $i++) { $total = sym_target($total) + $map[\"alpha\"]; } \
                      echo $total;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            inline_caches: InlineCacheMode::On,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"36");
    let counters = result.counters.expect("counters");
    assert!(counters.symbolized_call_name_hits >= 6, "{counters:?}");
    assert!(counters.symbolized_array_key_hits >= 6, "{counters:?}");
    assert_eq!(
        counters
            .symbolized_name_fallbacks_by_reason
            .get("uninterned_call_name"),
        None,
        "{counters:?}"
    );
    assert!(counters.function_call_ic_hits >= 1, "{counters:?}");
}

#[test]
fn foreach_by_value_clones_elements_not_snapshots() {
    // 64 iterations over a 64-element array: cloning the whole snapshot
    // per step costs at least 64*64 value clones; in-place stepping
    // keeps the total for the entire program far below that.
    let elements = (0..64).map(|i| i.to_string()).collect::<Vec<_>>();
    let source = format!(
        "<?php $a = [{}]; $sum = 0; foreach ($a as $v) {{ $sum += $v; }} echo $sum;",
        elements.join(", ")
    );
    for format in [ExecutionFormat::Ir, ExecutionFormat::Bytecode] {
        let result = execute_source_with_options(
            &source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                execution_format: format,
                ..VmOptions::default()
            },
        );
        assert!(
            result.status.is_success(),
            "{format:?}: {:?}",
            result.status
        );
        assert_eq!(result.output.as_bytes(), b"2016", "{format:?}");
        let counters = result.counters.expect("counters");
        assert!(
            counters.value_clones < 1500,
            "{format:?}: foreach should not clone the snapshot per step: {}",
            counters.value_clones
        );
        assert_eq!(
            counters
                .value_clone_by_reason
                .get(layout_source::FOREACH_VALUE.name())
                .copied(),
            Some(64),
            "{format:?}: {:?}",
            counters.value_clone_by_reason
        );
    }
}

#[test]
fn discarded_register_values_are_consumed_without_array_handle_clone() {
    let source = "<?php
            function discard_source() { return [1, 2, 3]; }
            for ($i = 0; $i < 32; $i++) { discard_source(); }
            echo 'ok';
        ";
    for format in [ExecutionFormat::Ir, ExecutionFormat::Bytecode] {
        let result = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                execution_format: format,
                ..VmOptions::default()
            },
        );
        assert!(
            result.status.is_success(),
            "{format:?}: {:?}",
            result.status
        );
        assert_eq!(result.output.as_bytes(), b"ok", "{format:?}");
        let counters = result.counters.expect("counters");
        assert!(
            counters.array_handle_clones < 48,
            "{format:?}: discarded array returns should not clone their register value: {}",
            counters.array_handle_clones
        );
    }
}

#[test]
fn dense_array_layout_probes_borrow_local_handles() {
    let result = execute_source_with_options(
        "<?php $values = []; for ($i = 0; $i < 32; $i++) { $values[] = $i; } echo count($values);",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"32");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters
            .array_handle_clone_by_source_family
            .get(layout_source::ARRAY_ELEMENT_WRITE.name())
            .copied()
            .unwrap_or_default(),
        0,
        "packed-layout probes must borrow the local array: {counters:?}"
    );
}

#[test]
fn rich_echo_borrows_register_operand_for_fast_scalar_output() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        collect_profile_spans: false,
        execution_format: ExecutionFormat::Ir,
        verify_ir: false,
        ..VmOptions::default()
    })
    .execute(manual_echo_unit(IrConstant::String(
        "borrowed echo".to_string(),
    )));

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"borrowed echo");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters
            .value_clone_by_source_family
            .get(layout_source::STACK_REGISTER_LOCAL_MOVE.name())
            .copied()
            .unwrap_or_default(),
        0,
        "{counters:?}"
    );
}

#[test]
fn by_ref_arg_bindings_attribute_materialization_and_cow_state() {
    let source = "<?php\n\
            function add_row(array &$rows, int $row): void { $rows[] = $row; }\n\
            function bump(int &$n): void { $n++; }\n\
            $rows = [];\n\
            for ($i = 0; $i < 4; $i++) { add_row($rows, $i); }\n\
            $n = 0;\n\
            bump($n);\n\
            echo count($rows), ':', $n;";
    for format in [ExecutionFormat::Ir, ExecutionFormat::Bytecode] {
        let result = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                execution_format: format,
                ..VmOptions::default()
            },
        );
        assert!(
            result.status.is_success(),
            "{format:?}: {:?}",
            result.status
        );
        assert_eq!(result.output.as_bytes(), b"4:1", "{format:?}");
        let counters = result.counters.expect("counters");
        assert_eq!(
            counters.by_ref_arg_location_binding_attempts, 5,
            "{format:?}: {counters:?}"
        );
        assert_eq!(
            counters.by_ref_arg_location_bindings, 5,
            "{format:?}: {counters:?}"
        );
        // Location encoding binds these calls through the caller local
        // slot: nothing is materialized, no array handle is pinned, and
        // every binding avoids a guaranteed copy-on-write separation.
        assert_eq!(
            counters.by_ref_arg_value_materializations, 0,
            "{format:?}: {counters:?}"
        );
        assert_eq!(
            counters.by_ref_arg_register_pins, 0,
            "{format:?}: {counters:?}"
        );
        assert_eq!(
            counters.by_ref_arg_cow_separations, 0,
            "{format:?}: {counters:?}"
        );
        assert_eq!(
            counters.by_ref_arg_cow_separations_avoided, 5,
            "{format:?}: {counters:?}"
        );
        assert_eq!(
            counters
                .by_ref_arg_fallback_by_reason
                .get("local_value_materialized")
                .copied(),
            None,
            "{format:?}: {:?}",
            counters.by_ref_arg_fallback_by_reason
        );
    }
}

#[test]
fn dense_callers_thread_method_bodies_through_the_plan() {
    let source = "<?php\n\
            class Counter {\n\
                private int $total = 0;\n\
                public function add(int $n): int { $this->total += $n; return $this->total; }\n\
                public function stream(): \\Generator { yield $this->total; }\n\
            }\n\
            $c = new Counter();\n\
            $sum = 0;\n\
            for ($i = 0; $i < 8; $i++) { $sum = $c->add($i); }\n\
            foreach ($c->stream() as $v) { $sum += $v; }\n\
            echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"56");
    let counters = result.counters.expect("counters");
    assert!(
        counters.dense_method_dispatch_hits >= 8,
        "method bodies should execute dense: {counters:?}"
    );
    // The generator method keeps its rich-fallback plan and the fallback
    // is attributed instead of silently dropping to rich execution.
    assert!(
        counters.rich_method_calls_from_dense_callers >= 1,
        "{counters:?}"
    );
    assert!(
        counters
            .dense_method_dispatch_fallback_by_reason
            .keys()
            .next()
            .is_some(),
        "{:?}",
        counters.dense_method_dispatch_fallback_by_reason
    );
}

#[test]
fn dense_jump_threading_is_output_identical_and_attributed() {
    let source = "<?php\n\
            $total = 0;\n\
            for ($i = 0; $i < 16; $i++) {\n\
                if ($i % 2 === 0) { $total += $i; } else { $total -= 1; }\n\
                while ($total > 40) { $total -= 5; }\n\
            }\n\
            echo $total;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            dense_jump_threading: DenseJumpThreadingMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            dense_jump_threading: DenseJumpThreadingMode::On,
            ..VmOptions::default()
        },
    );
    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(off.output.as_bytes(), on.output.as_bytes());
    let off_counters = off.counters.expect("counters");
    let on_counters = on.counters.expect("counters");
    assert_eq!(off_counters.dense_jump_threading_trampoline_blocks, 0);
    assert_eq!(on_counters.dense_jump_threading_rollbacks, 0);
    // The pass only rewrites explicit edges; whether any thread on this
    // shape is corpus-dependent, but attribution must always be present
    // when the pass is enabled and never when disabled.
    assert_eq!(off_counters.dense_jump_threading_threaded_edges, 0);
}

#[test]
fn const_pair_and_const_array_insert_fusions_are_output_identical() {
    let source = "<?php\n\
            $total = 0;\n\
            $rows = [];\n\
            for ($i = 0; $i < 12; $i++) {\n\
                $a = 3;\n\
                $b = 7;\n\
                $row = ['fixed', 42, 'tail'];\n\
                $rows['k' . $i] = 42;\n\
                $total += $a + $b + $row[1];\n\
            }\n\
            echo $total, ':', count($rows);";
    let unfused = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            superinstructions: SuperinstructionMode::Off,
            ..VmOptions::default()
        },
    );
    let fused = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            superinstructions: SuperinstructionMode::On,
            ..VmOptions::default()
        },
    );
    assert!(unfused.status.is_success(), "{:?}", unfused.status);
    assert!(fused.status.is_success(), "{:?}", fused.status);
    assert_eq!(unfused.output.as_bytes(), fused.output.as_bytes());
    let counters = fused.counters.expect("counters");
    assert!(
        counters
            .superinstructions_executed
            .get("load_const_load_const")
            .copied()
            .unwrap_or_default()
            > 0,
        "{:?}",
        counters.superinstructions_executed
    );
    assert!(
        counters
            .superinstructions_executed
            .get("load_const_array_insert")
            .copied()
            .unwrap_or_default()
            > 0,
        "{:?}",
        counters.superinstructions_executed
    );
    let unfused_counters = unfused.counters.expect("counters");
    assert!(
        counters.bytecode_instructions_executed < unfused_counters.bytecode_instructions_executed,
        "fusions must retire dispatches: {} vs {}",
        counters.bytecode_instructions_executed,
        unfused_counters.bytecode_instructions_executed
    );
}

#[test]
fn quickening_is_default_off_and_on_is_output_identical() {
    let source =
        "<?php $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $i; } echo $sum, \"\\n\";";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let off_counters = off.counters.expect("off counters");
    let on_counters = on.counters.expect("on counters");
    assert_eq!(off_counters.quickening_attempts, 0);
    assert!(on_counters.quickening_attempts > 0, "{on_counters:?}");
    assert!(on_counters.quickening_specialized > 0, "{on_counters:?}");
    assert!(on_counters.quickening_guard_hits > 0, "{on_counters:?}");
    assert_eq!(on_counters.quickening_guard_misses, 0);
    assert_eq!(on_counters.quickening_guard_failures, 0);
    assert_eq!(on_counters.quickening_dequickens, 0);
}

#[test]
fn quickening_observation_skips_non_candidate_rich_instructions() {
    // Twelve foreach iterations cross the per-site specialization
    // threshold, but no instruction kind here has a candidate arm, so no
    // site may report attempts or a phantom specialization.
    let source = "<?php foreach ([\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\", \"h\", \"i\", \"j\", \"k\", \"l\"] as $item) { echo $item; }";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"abcdefghijkl");
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("on counters");
    assert!(counters.instructions_executed > 12, "{counters:?}");
    assert_eq!(counters.quickening_attempts, 0, "{counters:?}");
    assert_eq!(counters.quickening_specialized, 0, "{counters:?}");
    assert_eq!(counters.quickening_guard_hits, 0, "{counters:?}");
}

#[test]
fn quickening_observation_skips_non_candidate_dense_instructions() {
    // Dense loads and echoes have no specialization arm. They must not
    // allocate or update quickening sites merely because a unit contains many
    // of them.
    let source = "<?php echo \"a\"; echo \"b\"; echo \"c\"; echo \"d\"; echo \"e\"; echo \"f\"; echo \"g\"; echo \"h\"; echo \"i\"; echo \"j\"; echo \"k\"; echo \"l\";";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"abcdefghijkl");
    let counters = result.counters.expect("counters");
    assert!(counters.bytecode_instructions_executed > 12, "{counters:?}");
    assert_eq!(counters.quickening_attempts, 0, "{counters:?}");
    assert_eq!(counters.quickening_specialized, 0, "{counters:?}");
    assert_eq!(counters.quickening_guard_hits, 0, "{counters:?}");
}

#[test]
fn tiering_disabled_suppresses_quickening_observations() {
    let source =
        "<?php $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $i; } echo $sum, \"\\n\";";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            tiering: TieringOptions {
                enabled: false,
                collect_stats: true,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"66\n");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.quickening_attempts, 0, "{counters:?}");
    let stats = result.tiering_stats.expect("tiering stats");
    assert!(stats.tiering_disabled_entries > 0, "{stats:?}");
    assert_eq!(stats.tier1_quickened_entries, 0, "{stats:?}");
    assert_eq!(stats.tier2_jit_candidates, 0, "{stats:?}");
}

#[test]
fn adaptive_tiny_unit_setup_skip_suppresses_quickening_observations() {
    let result = execute_source_with_options(
        "<?php echo \"tiny\\n\";",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            tiering: TieringOptions::default(),
            adaptive_tiny_unit_setup_threshold: Some(32),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"tiny\n");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.adaptive_tiny_unit_setup_skips, 1, "{counters:?}");
    assert_eq!(counters.quickening_attempts, 0, "{counters:?}");
}

#[test]
fn adaptive_tiny_unit_setup_keeps_larger_units_fast() {
    let source =
        "<?php $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $i; } echo $sum, \"\\n\";";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            tiering: TieringOptions::default(),
            adaptive_tiny_unit_setup_threshold: Some(1),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"66\n");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.adaptive_tiny_unit_setup_skips, 0, "{counters:?}");
    assert!(counters.quickening_attempts > 0, "{counters:?}");
}

#[cfg(not(feature = "jit-cranelift"))]
#[test]
fn managed_native_platform_unavailable_keeps_interpreter_fast_paths() {
    let source = "<?php function native_default_leaf(int $a, int $b): int { return $a + $b; } for ($i = 0; $i < 12; $i = $i + 1) { echo native_default_leaf($i, 2); }";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Ir,
            quickening: QuickeningMode::On,
            inline_caches: InlineCacheMode::On,
            jit: JitMode::Cranelift,
            // This test isolates the unavailable Cranelift tier. On aarch64,
            // No alternate native emitter may satisfy the generic native
            // execution counter here.
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2345678910111213");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert!(counters.native_candidates > 0, "{counters:?}");
    assert!(counters.native_platform_unavailable > 0, "{counters:?}");
    assert_eq!(counters.native_compiled_regions, 0, "{counters:?}");
    assert_eq!(counters.native_executions, 0, "{counters:?}");
    assert!(counters.quickening_attempts > 0, "{counters:?}");
    assert!(counters.inline_cache_observations > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn jit_int_leaf_hot_loop_executes_after_warmup() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + perf_jit_add($i, 2); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            // This test proves the Cranelift tier compiles and executes; the
            // This fixture isolates Cranelift's scalar-int lowering.
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"90");
    let counters = result.counters.expect("counters");
    assert!(counters.jit_compile_attempts > 0, "{counters:?}");
    assert!(counters.jit_compiled > 0, "{counters:?}");
    assert!(counters.jit_executed > 0, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn dense_cranelift_entry_marshals_direct_callee_slots() {
    let source = "<?php function dense_jit_add(int $a, int $b): int { return $a + $b; } $sum = 0; for ($i = 0; $i < 16; $i++) { $sum += dense_jit_add($i, 2); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Bytecode,
            jit: JitMode::Cranelift,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"152");
    let counters = result.counters.expect("counters");
    assert!(counters.cranelift_direct_slot_marshals > 0, "{counters:?}");
    assert_eq!(
        counters.cranelift_prepared_arg_materializations, 0,
        "{counters:?}"
    );
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_default_tiering_keeps_cold_function_interpreted() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } echo perf_jit_add(1, 2);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 0, "{counters:?}");
    assert_eq!(counters.jit_compiled, 0, "{counters:?}");
    assert!(counters.jit_tiering_cold_functions > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_threshold_tiering_compiles_hot_function() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } $sum = 0; for ($i = 0; $i < 4; $i++) { $sum += perf_jit_add($i, 2); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            jit_threshold: 2,
            tiering: TieringOptions {
                function_entry_threshold: 2,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"14");
    let counters = result.counters.expect("counters");
    assert!(counters.jit_compile_attempts > 0, "{counters:?}");
    assert!(counters.jit_compiled > 0, "{counters:?}");
    assert!(counters.jit_tiering_cold_functions > 0, "{counters:?}");
    assert!(counters.jit_tiering_hot_functions > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_eager_tiering_compiles_first_call_for_tests() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } echo perf_jit_add(5, 7);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            jit_threshold: 1,
            tiering: TieringOptions {
                jit_eager: true,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12");
    let counters = result.counters.expect("counters");
    assert!(counters.jit_compile_attempts > 0, "{counters:?}");
    assert!(counters.jit_compiled > 0, "{counters:?}");
    assert!(counters.jit_tiering_eager_functions > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_compile_budget_rejection_falls_back_to_interpreter() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } echo perf_jit_add(5, 7);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            jit_threshold: 1,
            tiering: TieringOptions {
                jit_eager: true,
                jit_max_functions: 0,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 0, "{counters:?}");
    assert_eq!(counters.jit_compiled, 0, "{counters:?}");
    assert!(counters.jit_tiering_budget_rejections > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_inline_arithmetic_executes_native_and_counts_fast_paths() {
    let source = "<?php function perf_jit_add_mul(int $a): int { return ($a + 2) * 3; } echo perf_jit_add_mul(4);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"18");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
    assert_eq!(counters.jit_helper_calls, 0, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 2, "{counters:?}");
    assert_eq!(counters.jit_overflow_exits, 0, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 0, "{counters:?}");
    assert!(counters.jit_code_bytes > 0, "{counters:?}");
    assert!(counters.jit_compile_time_nanos > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_same_unit_wrapper_stays_native_across_direct_call() {
    let source = "<?php function perf_native_increment(int $value): int { return $value + 1; } function perf_native_wrapper(int $value): int { return perf_native_increment($value); } echo perf_native_wrapper(41);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.compiled_to_compiled_calls, 1, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_record_lookup_executes_native_and_counts_fast_hit() {
    let source = "<?php function perf_record_lookup(array $m, string $k) { return $m[$k]; } $m = [\"host\" => \"db.local\", \"port\" => 5432]; echo perf_record_lookup($m, \"host\"), \":\", perf_record_lookup($m, \"port\");";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"db.local:5432");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert!(counters.record_lookup_fast_hits >= 1, "{counters:?}");
    assert_eq!(counters.record_lookup_key_miss_exits, 0, "{counters:?}");
    assert_eq!(counters.record_lookup_layout_exits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_record_lookup_key_miss_side_exits_and_resumes_interpreter() {
    // The missing key must side-exit and resume through the interpreter,
    // reproducing the undefined-key warning and null result exactly.
    let source = "<?php function perf_record_lookup_miss(array $m, string $k) { return $m[$k]; } $m = [\"present\" => 1]; var_dump(perf_record_lookup_miss($m, \"absent\"));";
    let native = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );
    let interpreted = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(native.status.is_success(), "{:?}", native.status);
    assert_eq!(native.output.as_bytes(), interpreted.output.as_bytes());
    let counters = native.counters.expect("counters");
    assert!(counters.record_lookup_key_miss_exits >= 1, "{counters:?}");
    assert_eq!(counters.record_lookup_fast_hits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_record_lookup_layout_exit_falls_back_for_non_record_array() {
    // A packed (non-record) array fails the record-shape guard; the
    // interpreter fallback still produces the exact value.
    let source = "<?php function perf_record_lookup_layout(array $m, string $k) { return $m[$k]; } $m = [7, 8, 9]; $m[\"late\"] = 10; echo perf_record_lookup_layout($m, \"late\");";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"10");
    let counters = result.counters.expect("counters");
    assert!(counters.record_lookup_layout_exits >= 1, "{counters:?}");
    assert_eq!(counters.record_lookup_fast_hits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_record_lookup_reference_slot_side_exits() {
    // A reference-holding slot violates the read-only guard; the
    // interpreter fallback preserves reference semantics.
    let source = "<?php function perf_record_lookup_ref(array $m, string $k) { return $m[$k]; } $cell = 41; $m = [\"cell\" => &$cell]; $cell = 42; echo perf_record_lookup_ref($m, \"cell\");";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42");
    let counters = result.counters.expect("counters");
    assert!(counters.record_lookup_layout_exits >= 1, "{counters:?}");
    assert_eq!(counters.record_lookup_fast_hits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_array_fetch_executes_native_and_counts_fast_hit() {
    let source = "<?php function perf_packed_fetch(array $xs, int $i): int { return $xs[$i]; } $xs = [10, 20, 30]; echo perf_packed_fetch($xs, 1);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            // This test pins the Cranelift packed-fetch implementation.
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"20");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_fast_hits, 1, "{counters:?}");
    assert_eq!(counters.packed_fetch_bounds_exits, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_layout_exits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_array_fetch_bounds_exit_falls_back_to_interpreter() {
    let source = "<?php function perf_packed_fetch_bounds(array $xs, int $i): int { return $xs[$i]; } echo perf_packed_fetch_bounds([10], 4);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_fast_hits, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_bounds_exits, 1, "{counters:?}");
    assert_eq!(counters.packed_fetch_layout_exits, 0, "{counters:?}");
    assert_eq!(counters.jit_side_exit_reasons.get("overflow"), None);
    assert_eq!(
        counters.jit_side_exit_reasons.get("helper_status"),
        Some(&1)
    );
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_array_fetch_layout_exit_falls_back_for_mixed_array() {
    let source = "<?php function perf_packed_fetch_mixed(array $xs, int $i): int { return $xs[$i]; } $xs = [0 => 11, 'name' => 12]; echo perf_packed_fetch_mixed($xs, 0);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_fast_hits, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_bounds_exits, 0, "{counters:?}");
    assert_eq!(counters.packed_fetch_layout_exits, 1, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_foreach_sum_executes_native_and_counts_fast_hit() {
    let source = "<?php function perf_packed_foreach_sum(array $xs): int { $sum = 0; foreach ($xs as $x) { $sum += $x; } return $sum; } echo perf_packed_foreach_sum([10, 20, 30]);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"60");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 1, "{counters:?}");
    assert_eq!(counters.jit_helper_calls, 0, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_fast_hits, 1, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_layout_exits, 0, "{counters:?}");
    assert_eq!(
        counters.packed_foreach_sum_overflow_exits, 0,
        "{counters:?}"
    );
    assert_eq!(counters.jit_slow_path_calls, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_foreach_sum_layout_exit_falls_back_for_mixed_element() {
    let source = "<?php function perf_packed_foreach_sum_mixed(array $xs): int { $sum = 0; foreach ($xs as $x) { $sum += $x; } return $sum; } echo perf_packed_foreach_sum_mixed([10, \"20\", 30]);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 1, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_fast_hits, 0, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_layout_exits, 1, "{counters:?}");
    assert_eq!(
        counters.packed_foreach_sum_overflow_exits, 0,
        "{counters:?}"
    );
    assert_eq!(
        counters.jit_side_exit_reasons.get("helper_status"),
        Some(&1)
    );
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_packed_foreach_sum_overflow_exit_falls_back_to_interpreter() {
    let source = "<?php function perf_packed_foreach_sum_overflow(array $xs): int { $sum = 0; foreach ($xs as $x) { $sum += $x; } return $sum; } echo perf_packed_foreach_sum_overflow([9223372036854775807, 1]);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 1, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_fast_hits, 0, "{counters:?}");
    assert_eq!(counters.packed_foreach_sum_layout_exits, 0, "{counters:?}");
    assert_eq!(
        counters.packed_foreach_sum_overflow_exits, 1,
        "{counters:?}"
    );
    assert_eq!(counters.jit_overflow_exits, 1, "{counters:?}");
    assert_eq!(counters.jit_side_exit_reasons.get("overflow"), Some(&1));
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_known_strlen_executes_native_and_counts_fast_hit() {
    let source = "<?php function perf_known_strlen(string $s): int { return strlen($s); } echo perf_known_strlen(\"hello\");";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_helper_calls, 1, "{counters:?}");
    assert_eq!(counters.known_call_fast_hits, 1, "{counters:?}");
    assert_eq!(counters.known_call_guard_exits, 0, "{counters:?}");
    assert_eq!(counters.known_call_slow_calls, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_known_strlen_guard_exit_preserves_type_error() {
    let source = "<?php function perf_known_strlen_guard($s): int { return strlen($s); } try { echo perf_known_strlen_guard([]); } catch (TypeError $e) { echo \"type-error\"; }";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.known_call_fast_hits, 0, "{counters:?}");
    assert_eq!(counters.known_call_guard_exits, 1, "{counters:?}");
    assert_eq!(counters.known_call_slow_calls, 1, "{counters:?}");
    assert_eq!(
        counters.jit_side_exit_reasons.get("helper_status"),
        Some(&1)
    );
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_known_count_executes_for_packed_and_mixed_arrays() {
    let source = "<?php function perf_known_count(array $xs): int { return count($xs); } echo perf_known_count([10, 20, 30]), \":\", perf_known_count([\"a\" => 1, 4 => 2]);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3:2");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 2, "{counters:?}");
    assert_eq!(counters.jit_helper_calls, 2, "{counters:?}");
    assert_eq!(counters.known_call_fast_hits, 2, "{counters:?}");
    assert_eq!(counters.known_call_guard_exits, 0, "{counters:?}");
    assert_eq!(counters.known_call_slow_calls, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_string_concat_executes_native_and_counts_fast_hit() {
    let source = "<?php function perf_string_concat(string $a, string $b): string { return $a . $b; } echo perf_string_concat(\"hello\", \"-world\");";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"hello-world");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_helper_calls, 1, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 1, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_hits, 1, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_misses, 0, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_string_concat_rejects_string_int_without_fast_hit() {
    let source = "<?php function perf_string_int_concat(string $a, int $b): string { return $a . $b; } echo perf_string_int_concat(\"id-\", 42);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    assert_eq!(on.output.as_bytes(), b"id-42");
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 0, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_hits, 0, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_misses, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_string_concat_rejects_object_to_string_without_fast_hit() {
    let source = "<?php class PerfConcatObject { public function __toString(): string { return 'object'; } } function perf_object_concat($a, $b): string { return $a . $b; } echo perf_object_concat(new PerfConcatObject(), '-tail');";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    assert_eq!(on.output.as_bytes(), b"object-tail");
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 0, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_hits, 0, "{counters:?}");
    assert_eq!(counters.string_concat_fast_path_misses, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_helper_overflow_status_falls_back_to_interpreter() {
    let source = "<?php function perf_jit_overflow(int $a): int { return $a + 1; } echo perf_jit_overflow(9223372036854775807);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert!(counters.jit_bailouts > 0, "{counters:?}");
    assert_eq!(counters.jit_side_exits, 1, "{counters:?}");
    assert_eq!(
        counters.jit_side_exit_reasons.get("overflow"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(counters.jit_overflow_exits, 1, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 1, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_type_switch_side_exits_do_not_blacklist_from_tiny_sample() {
    let source = r#"<?php
function perf_jit_unstable_types(int $a): int { return $a + 1; }
echo perf_jit_unstable_types(1), "\n";
echo perf_jit_unstable_types("2"), "\n";
echo perf_jit_unstable_types("3"), "\n";
echo perf_jit_unstable_types(4), "\n";
"#;
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 2, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 2, "{counters:?}");
    assert_eq!(counters.jit_side_exits, 2, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 2, "{counters:?}");
    assert_eq!(
        counters.jit_side_exit_reasons.get("type_mismatch"),
        Some(&2),
        "{counters:?}"
    );
    assert_eq!(counters.jit_blacklisted_regions, 0, "{counters:?}");
    assert!(counters.jit_blacklist_reasons.is_empty(), "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_blacklist_can_be_disabled_for_debugging() {
    let source = r#"<?php
function perf_jit_unstable_types_debug(int $a): int { return $a + 1; }
echo perf_jit_unstable_types_debug(1), "\n";
echo perf_jit_unstable_types_debug("2"), "\n";
echo perf_jit_unstable_types_debug("3"), "\n";
echo perf_jit_unstable_types_debug(4), "\n";
"#;
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            jit_blacklist: JitBlacklistMode::Off,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2\n3\n4\n5\n");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 2, "{counters:?}");
    assert_eq!(counters.jit_fast_path_hits, 2, "{counters:?}");
    assert_eq!(counters.jit_side_exits, 2, "{counters:?}");
    assert_eq!(counters.jit_slow_path_calls, 2, "{counters:?}");
    assert_eq!(counters.jit_blacklisted_regions, 0, "{counters:?}");
    assert!(counters.jit_blacklist_reasons.is_empty(), "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_constant_return_executes_native_after_abi_check() {
    let source = "<?php function perf_const(): int { return 42; } echo perf_const();";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_mode, "cranelift");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 1, "{counters:?}");
    assert_eq!(counters.jit_bailouts, 0, "{counters:?}");
    assert!(counters.jit_code_bytes > 0, "{counters:?}");
    assert!(counters.jit_compile_time_nanos > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_compile_cache_reuses_same_function() {
    let source = "<?php function perf_const(): int { return 42; } echo perf_const(); echo perf_const(); echo perf_const();";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                jit_eager: true,
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"424242");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.jit_compile_attempts, 1, "{counters:?}");
    assert_eq!(counters.jit_compiled, 1, "{counters:?}");
    assert_eq!(counters.jit_executed, 3, "{counters:?}");
    assert_eq!(counters.jit_compile_cache_misses, 1, "{counters:?}");
    assert_eq!(counters.jit_compile_cache_hits, 2, "{counters:?}");
    assert_eq!(counters.jit_compile_cache_invalidations, 0, "{counters:?}");
    assert_eq!(
        counters.jit_process_cache_hits + counters.jit_process_cache_misses,
        1,
        "{counters:?}"
    );
    assert!(counters.jit_code_bytes_live > 0, "{counters:?}");
    assert!(counters.jit_code_generations > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_compile_cache_rejects_changed_ir_and_abi() {
    let mut cache = JitRuntimeState::default();
    let function = FunctionId::new(0);
    let base = JitCompileCacheKey {
        function: function.raw(),
        ir_fingerprint: 11,
        abi_hash: php_jit::JIT_RUNTIME_ABI_HASH,
        config_hash: 22,
        target_isa: "test-target".to_owned(),
    };
    let handle = php_jit::JitFunctionHandle::metadata_only(
        1,
        "function.perf_const".to_owned(),
        php_jit::JitBackend::CraneliftExperiment,
    );
    cache.insert_compile_cache(base.clone(), handle, 1);

    assert!(matches!(
        cache.lookup_compile_cache(&base, 1),
        JitCompileCacheLookup::Hit(_)
    ));

    let mut changed_ir = base.clone();
    changed_ir.ir_fingerprint = 12;
    assert_eq!(
        cache.lookup_compile_cache(&changed_ir, 1),
        JitCompileCacheLookup::Miss
    );

    let mut changed_abi = base.clone();
    changed_abi.abi_hash = php_jit::JIT_RUNTIME_ABI_HASH.wrapping_add(1);
    assert_eq!(
        cache.lookup_compile_cache(&changed_abi, 1),
        JitCompileCacheLookup::Miss
    );
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_compile_cache_keeps_layout_independent_handle_across_epoch() {
    let mut cache = JitRuntimeState::default();
    let function = FunctionId::new(0);
    let key = JitCompileCacheKey {
        function: function.raw(),
        ir_fingerprint: 11,
        abi_hash: php_jit::JIT_RUNTIME_ABI_HASH,
        config_hash: 22,
        target_isa: "test-target".to_owned(),
    };
    let handle = php_jit::JitFunctionHandle::metadata_only(
        1,
        "function.perf_const".to_owned(),
        php_jit::JitBackend::CraneliftExperiment,
    );
    cache.insert_compile_cache(key.clone(), handle, 1);

    assert!(matches!(
        cache.lookup_compile_cache(&key, 2),
        JitCompileCacheLookup::Hit(_)
    ));
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn jit_rejected_leaf_falls_back_to_interpreter() {
    let source = "<?php function perf_jit_reject($value): int { $items = []; $items[] = strlen($value); return $items[0]; } $sum = 0; for ($i = 0; $i < 8; $i++) { $sum = $sum + perf_jit_reject('abcd'); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"32");
    let counters = result.counters.expect("counters");
    assert!(counters.jit_compile_attempts > 0, "{counters:?}");
    assert_eq!(counters.jit_compiled, 0, "{counters:?}");
    assert_eq!(counters.jit_executed, 0, "{counters:?}");
    assert!(counters.jit_bailouts > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn jit_on_off_output_is_identical() {
    let source = "<?php function perf_jit_add(int $a, int $b): int { return $a + $b; } $sum = 0; for ($i = 0; $i < 10; $i++) { $sum = $sum + perf_jit_add($i, 3); } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            // Compare interpreter and Cranelift specifically.
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let off_counters = off.counters.expect("off counters");
    let on_counters = on.counters.expect("on counters");
    assert_eq!(off_counters.jit_compile_attempts, 0);
    assert!(on_counters.jit_executed > 0, "{on_counters:?}");
}

#[test]
fn inline_cache_slots_are_counted_without_changing_output() {
    let source = "<?php function ic_f() { return 1; } class ICSlotSmoke { public $x = 3; public function m() { return 2; } } $object = new ICSlotSmoke(); $items = [4,5]; for ($i = 0; $i < 3; $i++) { echo ic_f(), $object->m(), $object->x, $items[1]; }";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let off_counters = off.counters.expect("off counters");
    let on_counters = on.counters.expect("on counters");
    assert_eq!(off_counters.inline_cache_slots, 0);
    assert_eq!(off_counters.inline_cache_observations, 0);
    assert!(on_counters.inline_cache_slots >= 4, "{on_counters:?}");
    assert!(
        on_counters.inline_cache_observations >= on_counters.inline_cache_slots,
        "{on_counters:?}"
    );
    assert!(
        on_counters.inline_cache_function_slots > 0,
        "{on_counters:?}"
    );
    assert!(on_counters.inline_cache_method_slots > 0, "{on_counters:?}");
    assert!(
        on_counters.inline_cache_property_slots > 0,
        "{on_counters:?}"
    );
    assert!(on_counters.inline_cache_dim_slots > 0, "{on_counters:?}");
    assert!(on_counters.inline_cache_hits > 0, "{on_counters:?}");
    assert!(on_counters.inline_cache_misses > 0, "{on_counters:?}");
    assert_eq!(on_counters.inline_cache_invalidations, 0);
    assert_eq!(on_counters.inline_cache_guard_failures, 0);
    assert_eq!(on_counters.inline_cache_megamorphic, 0);
}

#[test]
fn function_call_inline_cache_records_user_function_hits() {
    let source = "<?php function perf_ic_user($value) { return $value + 1; } $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = perf_ic_user($sum); } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    let counters = on.counters.expect("on counters");
    assert!(counters.inline_cache_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_misses > 0, "{counters:?}");
    assert_eq!(counters.inline_cache_invalidations, 0);
    assert_eq!(counters.inline_cache_megamorphic, 0);
}

#[test]
fn function_call_inline_cache_records_internal_function_hits() {
    let source =
        "<?php $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + strlen('abcd'); } echo $sum;";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"48");
    let counters = on.counters.expect("on counters");
    assert!(counters.inline_cache_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_misses > 0, "{counters:?}");
}

#[test]
fn function_call_inline_cache_handles_namespaced_functions() {
    let source = "<?php namespace PerformanceIC; function hot() { return 2; } $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + hot(); } echo $sum;";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"24");
    let counters = on.counters.expect("on counters");
    assert!(counters.inline_cache_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_misses > 0, "{counters:?}");
}

#[test]
fn function_calls_fallback_to_global_internal_builtin_from_namespace() {
    let result = execute_source("<?php namespace Foo; echo strlen('abc');");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3");
}

#[test]
fn function_calls_fallback_to_global_class_alias_from_namespace() {
    let result = execute_source(
        "<?php namespace SimplePie; class SourceClass {} echo class_alias(SourceClass::class, 'SimplePie_Alias') ? 'alias' : 'missing';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"alias");
}

#[test]
fn function_calls_fallback_to_global_assert_from_namespace() {
    let result =
        execute_source("<?php namespace SimplePie; echo assert(true) ? 'asserted' : 'missing';");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"asserted");
}

#[test]
fn function_calls_fallback_to_global_mail_builtin_from_namespace() {
    let result = execute_source(
        "<?php namespace PHPMailer\\PHPMailer; echo mail('admin@example.test', 'Subject', 'Body', 'Header: value') ? 'sent' : 'failed';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"sent");
}

#[test]
fn function_calls_fallback_to_global_context_builtin_from_namespace() {
    let result =
        execute_source("<?php namespace Sodium; echo is_callable('strlen') ? 'callable' : 'no';");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"callable");
}

#[test]
fn function_calls_fallback_to_global_gc_enabled_builtin_from_namespace() {
    let result =
        execute_source("<?php namespace SimplePie; echo gc_enabled() ? 'gc-on' : 'gc-off';");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"gc-on");
}

#[test]
fn is_callable_named_out_parameter_does_not_warn_for_undefined_local() {
    let result = execute_source(
        "<?php function callable_name_helper() {} is_callable(callable_name_helper(...), callable_name: $name); var_dump($name);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"string(20) \"callable_name_helper\"\n"
    );
}

#[test]
fn function_calls_prefer_namespaced_user_function_over_context_builtin_fallback() {
    let result = execute_source(
        "<?php namespace Sodium; function is_callable($value) { return 'local'; } echo is_callable('strlen');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"local");
}

#[test]
fn function_calls_prefer_namespaced_user_function_over_builtin_fallback() {
    let result = execute_source(
        "<?php namespace Foo; function strlen($value) { return 7; } echo strlen('abc');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
}

#[test]
fn function_fetches_globals_dim_after_empty_probe() {
    let result = execute_source(
        "<?php $shortcode_tags = ['gallery' => true]; function f() { echo ! empty($GLOBALS['shortcode_tags']) ? array_keys($GLOBALS['shortcode_tags'])[0] : 'empty'; } f();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"gallery");
}

#[test]
fn dense_function_empty_globals_dim_controls_ternary() {
    let cases = [
        (
            "array",
            "<?php $shortcode_tags = ['gallery' => true]; function f() { echo ! empty($GLOBALS['shortcode_tags']) ? array_keys($GLOBALS['shortcode_tags'])[0] : 'empty'; } f();",
            b"gallery".as_slice(),
        ),
        (
            "null",
            "<?php $shortcode_tags = null; function f() { echo ! empty($GLOBALS['shortcode_tags']) ? array_keys($GLOBALS['shortcode_tags'])[0] : 'empty'; } f();",
            b"empty".as_slice(),
        ),
    ];

    for (name, source, expected) in cases {
        let result = execute_source_with_options(
            source,
            VmOptions {
                execution_format: ExecutionFormat::Auto,
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                ..VmOptions::default()
            },
        );

        assert!(result.status.is_success(), "{name}: {:?}", result.status);
        assert_eq!(result.output.as_bytes(), expected, "{name}");
        let counters = result.counters.expect("counters");
        assert!(
            counters.bytecode_instructions_executed > 0,
            "{name}: {counters:?}"
        );
    }
}

#[test]
fn function_call_inline_cache_invalidates_after_include_defined_function() {
    let result = execute_fixture_file_with_options(
        "tests/fixtures/performance/inline_cache/include-invalidation.php",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"111111111111:yes\n");
    let counters = result.counters.expect("counters");
    assert!(counters.inline_cache_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_misses > 0, "{counters:?}");
    assert!(counters.inline_cache_invalidations > 0, "{counters:?}");
}

#[test]
fn function_call_inline_cache_preserves_function_exists_introspection() {
    let source = "<?php function perf_ic_exists() { return 1; } for ($i = 0; $i < 12; $i++) { echo function_exists('strlen') ? 'b' : 'm'; echo function_exists('perf_ic_exists') ? 'u' : 'm'; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"bubububububububububububu");
    let counters = on.counters.expect("counters");
    assert!(counters.inline_cache_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_misses > 0, "{counters:?}");
}

#[test]
fn function_call_inline_cache_records_polymorphic_string_callable_hits() {
    let source = "<?php function perf_poly_call_a() { return 'A'; } function perf_poly_call_b() { return 'B'; } foreach (['perf_poly_call_a', 'perf_poly_call_b', 'perf_poly_call_a', 'perf_poly_call_b'] as $name) { echo $name(); }";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"ABAB");
    let counters = on.counters.expect("on counters");
    assert!(counters.function_call_ic_hits > 0, "{counters:?}");
    assert!(counters.function_call_ic_misses > 0, "{counters:?}");
    assert!(counters.inline_cache_polymorphic > 0, "{counters:?}");
    assert_eq!(counters.call_ic_megamorphic_fallbacks, 0, "{counters:?}");
    assert_eq!(counters.inline_cache_disabled, 0, "{counters:?}");
}

#[test]
fn function_call_inline_cache_megamorphic_string_callable_falls_back() {
    let source = "<?php function perf_mega_call_a() { return 'A'; } function perf_mega_call_b() { return 'B'; } function perf_mega_call_c() { return 'C'; } function perf_mega_call_d() { return 'D'; } function perf_mega_call_e() { return 'E'; } foreach (['perf_mega_call_a', 'perf_mega_call_b', 'perf_mega_call_c', 'perf_mega_call_d', 'perf_mega_call_e', 'perf_mega_call_a'] as $name) { echo $name(); }";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"ABCDEA");
    let counters = on.counters.expect("on counters");
    assert!(counters.function_call_ic_misses > 0, "{counters:?}");
    assert!(counters.inline_cache_megamorphic > 0, "{counters:?}");
    assert!(counters.call_ic_megamorphic_fallbacks > 0, "{counters:?}");
    assert_eq!(counters.inline_cache_disabled, 0, "{counters:?}");
}

#[test]
fn method_call_inline_cache_records_hot_loop_hits() {
    let source = "<?php class PerfMethodHot { public function value() { return 2; } } $object = new PerfMethodHot(); $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $object->value(); } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"24");
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert!(counters.method_ic_misses > 0, "{counters:?}");
    assert!(counters.method_direct_dispatch_hits > 0, "{counters:?}");
    assert_eq!(counters.method_ic_guard_failures, 0);
}

#[test]
fn method_call_inline_cache_handles_inherited_methods() {
    let source = "<?php class PerfMethodBase { public function value() { return 'B'; } } class PerfMethodChild extends PerfMethodBase {} $object = new PerfMethodChild(); for ($i = 0; $i < 8; $i++) { echo $object->value(); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"BBBBBBBB");
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert!(counters.method_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.method_ic_guard_failures, 0);
}

#[test]
fn method_call_inline_cache_guard_fails_for_overridden_receivers() {
    let source = "<?php class PerfMethodA { public function value() { return 'A'; } } class PerfMethodB extends PerfMethodA { public function value() { return 'B'; } } $flip = false; for ($i = 0; $i < 8; $i++) { if ($flip) { $object = new PerfMethodB(); $flip = false; } else { $object = new PerfMethodA(); $flip = true; } echo $object->value(); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"ABABABAB");
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_misses > 0, "{counters:?}");
    assert!(counters.method_ic_guard_failures > 0, "{counters:?}");
    assert!(
        counters.optimized_exit_snapshots_created >= counters.method_ic_guard_failures,
        "{counters:?}"
    );
    assert_eq!(
        counters.optimized_exit_snapshots_created,
        counters.optimized_exit_snapshots_materialized
    );
    assert!(
        counters.fallback_resume_successes >= counters.method_ic_guard_failures,
        "{counters:?}"
    );
    assert!(counters.method_ic_polymorphic_hits > 0, "{counters:?}");
    assert!(
        counters.method_direct_dispatch_fallbacks > 0,
        "{counters:?}"
    );
}

#[test]
fn method_call_inline_cache_preserves_magic_call_fallback() {
    let source = "<?php class PerfMagicMethod { public function __call($name, $args) { return $name . count($args); } } $object = new PerfMagicMethod(); for ($i = 0; $i < 4; $i++) { echo $object->missing(1, 2); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"missing2missing2missing2missing2");
    let counters = on.counters.expect("on counters");
    assert_eq!(counters.method_ic_hits, 0, "{counters:?}");
    assert!(counters.method_ic_misses > 0, "{counters:?}");
}

#[test]
fn method_call_inline_cache_keeps_private_and_protected_visibility() {
    let source = "<?php class PerfMethodScopeBase { private function secret() { return 's'; } protected function inherited() { return 'p'; } public function callSecret() { return $this->secret(); } } class PerfMethodScopeChild extends PerfMethodScopeBase { public function callProtected() { return $this->inherited(); } } $object = new PerfMethodScopeChild(); for ($i = 0; $i < 6; $i++) { echo $object->callSecret(), $object->callProtected(); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"spspspspspsp");
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert_eq!(counters.method_ic_guard_failures, 0, "{counters:?}");

    let private = execute_source_with_options(
        "<?php class PerfMethodPrivate { private function secret() { return 1; } } (new PerfMethodPrivate())->secret();",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert_eq!(private.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        private.output.to_string_lossy().contains(
            "Uncaught Error: Call to private method PerfMethodPrivate::secret() from global scope"
        ),
        "{}",
        private.output.to_string_lossy()
    );
}

#[test]
fn method_call_uses_calling_scope_private_method_before_child_override() {
    let source = "<?php class PrivateScopeBase { public function pub() { $this->priv(); } private function priv() { echo 'base'; } } class PrivateScopeChild extends PrivateScopeBase { public function priv() { echo 'child'; } } $object = new PrivateScopeChild(); for ($i = 0; $i < 4; $i++) { $object->pub(); $object->priv(); }";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"basechildbasechildbasechildbasechild"
    );
    let counters = result.counters.expect("counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert_eq!(counters.method_ic_guard_failures, 0, "{counters:?}");
}

#[test]
fn method_call_inline_cache_handles_trait_method_alias() {
    let source = "<?php trait PerfMethodTrait { public function base() { return 't'; } } class PerfMethodTraitUser { use PerfMethodTrait { base as alias; } } $object = new PerfMethodTraitUser(); for ($i = 0; $i < 8; $i++) { echo $object->alias(); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"tttttttt");
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert!(counters.method_ic_misses > 0, "{counters:?}");
}

#[test]
fn class_relation_cache_preserves_instanceof_and_method_semantics() {
    let source = r#"<?php
class PerfRelationBase { public function value(): string { return 'p'; } }
class PerfRelationChild extends PerfRelationBase {}
interface PerfRelationIface { public function iface(): string; }
class PerfRelationImpl implements PerfRelationIface { public function iface(): string { return 'i'; } }
trait PerfRelationTrait { public function traitValue(): string { return 't'; } }
class PerfRelationTraitUser { use PerfRelationTrait; }
class PerfRelationOverride extends PerfRelationBase { public function value(): string { return 'c'; } }
final class PerfRelationFinal { final public function value(): string { return 'f'; } }

$child = new PerfRelationChild();
$impl = new PerfRelationImpl();
$trait = new PerfRelationTraitUser();
$final = new PerfRelationFinal();
for ($i = 0; $i < 4; $i++) {
    echo ($child instanceof PerfRelationBase) ? 'T' : 'F';
    echo ($child instanceof PerfRelationIface) ? 'T' : 'F';
    echo ($impl instanceof PerfRelationIface) ? 'I' : 'N';
    echo $trait->traitValue();
    echo $final->value();
}
eval('$relationEpochTouch = 1;');
for ($i = 0; $i < 3; $i++) {
    echo ($child instanceof PerfRelationBase) ? 'T' : 'F';
}
foreach ([new PerfRelationBase(), new PerfRelationOverride(), new PerfRelationBase(), new PerfRelationOverride()] as $object) {
    echo $object->value();
}
"#;
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"TFItfTFItfTFItfTFItfTTTpcpc");
    let off_counters = off.counters.expect("off counters");
    assert_eq!(off_counters.class_relation_cache_hits, 0);
    assert_eq!(off_counters.instanceof_cache_hits, 0);
    assert_eq!(off_counters.method_override_cache_hits, 0);
    let counters = on.counters.expect("on counters");
    assert!(counters.class_relation_cache_hits > 0, "{counters:?}");
    assert!(counters.class_relation_cache_misses > 0, "{counters:?}");
    assert!(
        counters.class_relation_cache_invalidations > 0,
        "{counters:?}"
    );
    assert!(counters.instanceof_cache_hits > 0, "{counters:?}");
    assert!(counters.instanceof_cache_misses > 0, "{counters:?}");
    assert!(counters.method_override_cache_hits > 0, "{counters:?}");
    assert!(counters.method_override_cache_misses > 0, "{counters:?}");
}

#[test]
fn method_call_profiles_declared_monomorphic_final_method() {
    let source = "<?php class PerfMethodProfileFinal { final public function value(): int { return 7; } } $object = new PerfMethodProfileFinal(); $sum = 0; for ($i = 0; $i < 4; $i++) { $sum = $sum + $object->value(); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"28");
    let counters = result.counters.expect("counters");
    assert!(counters.method_tiny_inline_candidates > 0, "{counters:?}");
    let profile = method_call_profile(&counters, "value");
    assert_eq!(profile.receiver_classes.len(), 1, "{profile:?}");
    assert_eq!(
        profile
            .method_slot_indexes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert!(profile.saw_final_method, "{profile:?}");
    assert!(profile.simple_positional_arguments, "{profile:?}");
    assert!(!profile.saw_by_ref_argument, "{profile:?}");
    assert!(profile.saw_callee_jit_eligible, "{profile:?}");
    assert!(profile.non_eligible_reasons.is_empty(), "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"method_call_profiles\": ["), "{json}");
    assert!(json.contains("\"state\": \"monomorphic\""), "{json}");
    assert!(json.contains("\"fast_path_eligible\": true"), "{json}");
}

#[test]
fn method_call_profiles_subclass_override_is_not_monomorphic() {
    let source = "<?php class PerfMethodProfileBase { public function value(): int { return 1; } } class PerfMethodProfileChild extends PerfMethodProfileBase { public function value(): int { return 2; } } function perf_method_profile_value(PerfMethodProfileBase $object): int { return $object->value(); } echo perf_method_profile_value(new PerfMethodProfileBase()), perf_method_profile_value(new PerfMethodProfileChild());";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12");
    let counters = result.counters.expect("counters");
    let profile = method_call_profile(&counters, "value");
    assert_eq!(profile.receiver_classes.len(), 2, "{profile:?}");
    assert_eq!(profile.method_ids.len(), 2, "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"state\": \"polymorphic\""), "{json}");
    assert!(json.contains("\"polymorphic_receiver\""), "{json}");
    assert!(json.contains("\"unstable_method_slot\""), "{json}");
}

#[test]
fn method_call_profiles_magic_call_fallback() {
    let source = "<?php class PerfMethodProfileMagic { public function __call($name, $args): int { return 42; } } $object = new PerfMethodProfileMagic(); echo $object->missing(1, 2);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"42");
    let counters = result.counters.expect("counters");
    let profile = method_call_profile(&counters, "missing");
    assert!(profile.has_magic_call, "{profile:?}");
    assert!(profile.magic_call_fallback, "{profile:?}");
    assert!(profile.method_ids.is_empty(), "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"magic_call_fallback\""), "{json}");
    assert!(json.contains("\"missing_declared_method\""), "{json}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_method_direct_call_hits_after_initial_fallback() {
    let source = "<?php class PerfDirectMethodTest { public function value(int $x): int { return $x + 2; } } $object = new PerfDirectMethodTest(); $sum = 0; for ($i = 0; $i < 8; $i++) { $sum += $object->value($i); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"44");
    let counters = result.counters.expect("counters");
    assert!(counters.direct_call_hits > 0, "{counters:?}");
    assert!(counters.direct_call_fallbacks > 0, "{counters:?}");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_method_direct_call_subclass_receiver_falls_back() {
    let source = "<?php class PerfDirectBaseTest { public function value(int $x): int { return $x + 1; } } class PerfDirectChildTest extends PerfDirectBaseTest { public function value(int $x): int { return $x + 10; } } $objects = [new PerfDirectBaseTest(), new PerfDirectChildTest(), new PerfDirectBaseTest()]; $sum = 0; foreach ($objects as $i => $object) { $sum += $object->value($i); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"15");
    let counters = result.counters.expect("counters");
    assert!(counters.direct_call_hits > 0, "{counters:?}");
    assert!(counters.direct_call_fallbacks > 0, "{counters:?}");
    assert!(counters.method_ic_guard_failures > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_method_direct_call_magic_path_stays_fallback() {
    let source = "<?php class PerfDirectMagicTest { public function __call(string $name, array $args): int { return strlen($name) + $args[0]; } } $object = new PerfDirectMagicTest(); echo $object->missing(5);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.direct_call_hits, 0, "{counters:?}");
    assert!(counters.direct_call_fallbacks > 0, "{counters:?}");
}

#[cfg(feature = "jit-cranelift")]
#[test]
fn cranelift_method_direct_call_propagates_callee_exception() {
    let source = "<?php class PerfDirectThrowerTest { public int $calls = 0; public function fail(): int { $this->calls = $this->calls + 1; if ($this->calls > 1) { throw new Exception(\"performance-direct-method\"); } return $this->calls; } } $object = new PerfDirectThrowerTest(); for ($i = 0; $i < 2; $i++) { echo $object->fail(), \"\\n\"; }";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            jit: JitMode::Cranelift,
            tiering: TieringOptions {
                function_entry_threshold: 1,
                ..TieringOptions::default()
            },
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = String::from_utf8(result.output.as_bytes().to_vec()).expect("utf-8 output");
    assert!(
        output.starts_with("1\n\nFatal error: Uncaught Exception: performance-direct-method in "),
        "{output}"
    );
    assert!(output.contains("PerfDirectThrowerTest->fail()"), "{output}");
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("performance-direct-method")),
        "{:?}",
        result.status
    );
    let counters = result.counters.expect("counters");
    assert!(counters.direct_call_hits > 0, "{counters:?}");
    assert!(counters.direct_call_fallbacks > 0, "{counters:?}");
}

#[test]
fn property_fetch_inline_cache_records_public_hot_loop_hits() {
    let source = "<?php class PerfPropertyHot { public $value = 3; } $object = new PerfPropertyHot(); $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $object->value; } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"36");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_ic_hits > 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.property_ic_guard_failures, 0);
}

#[test]
fn property_assign_inline_cache_records_public_and_typed_hits() {
    let source = "<?php class PerfAssignPropertyHot { public int $value = 0; } $object = new PerfAssignPropertyHot(); for ($i = 0; $i < 12; $i++) { $object->value = $i; } echo $object->value;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"11");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_assign_ic_hits > 0, "{counters:?}");
    assert!(counters.property_assign_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.property_assign_ic_type_exits, 0, "{counters:?}");
    assert_eq!(
        counters.property_assign_ic_readonly_exits, 0,
        "{counters:?}"
    );
    assert_eq!(
        counters.property_assign_ic_hook_magic_exits, 0,
        "{counters:?}"
    );
    assert_eq!(
        counters.property_assign_ic_reference_exits, 0,
        "{counters:?}"
    );
}

#[test]
fn property_assign_inline_cache_records_required_fallback_reasons() {
    let source = "<?php class PerfAssignTyped { public int $value = 0; } class PerfAssignDynamic {} class PerfAssignMagic { public int $seen = 0; public function __set(string $name, $value): void { $this->seen = $value; } } class PerfAssignHook { public string $name { set { $this->name = strtoupper($value); } get { return $this->name; } } } class PerfAssignPrivate { private int $value = 1; } $total = 0; $typed = new PerfAssignTyped(); try { $typed->value = 'bad'; } catch (Throwable $e) { $total += 1; } $dynamic = new PerfAssignDynamic(); $dynamic->value = 3; $total += $dynamic->value; $magic = new PerfAssignMagic(); $magic->missing = 4; $total += $magic->seen; $hook = new PerfAssignHook(); $hook->name = 'ada'; $total += strlen($hook->name); $private = new PerfAssignPrivate(); try { $private->value = 5; } catch (Throwable $e) { $total += 5; } echo $total;";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"16");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_assign_ic_misses > 0, "{counters:?}");
    assert!(
        counters.property_assign_ic_visibility_exits > 0,
        "{counters:?}"
    );
    assert!(counters.property_assign_ic_type_exits > 0, "{counters:?}");
    assert!(
        counters.property_assign_ic_hook_magic_exits > 0,
        "{counters:?}"
    );
    assert!(
        counters.property_assign_ic_dynamic_exits > 0,
        "{counters:?}"
    );
    for reason in [
        "visibility_mismatch",
        "type_mismatch",
        "property_hook_present",
        "dynamic_property_fallback",
        "magic_set_metadata",
    ] {
        assert!(
            counters
                .property_assign_ic_fallback_reasons
                .contains_key(reason),
            "missing {reason}: {counters:?}"
        );
    }
}

#[test]
fn property_assign_inline_cache_records_readonly_error_fallback() {
    let source = "<?php class PerfAssignReadonly { public readonly int $value; public function init(): void { $this->value = 1; } } $readonly = new PerfAssignReadonly(); $readonly->init(); $readonly->value = 2;";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status.exit_status(), ExitStatus::RuntimeError);
    // Readonly violations raise a catchable Error; uncaught at top
    // level it surfaces through the uncaught-exception path.
    assert!(
        on.status.message().is_some_and(
            |message| message.contains("Uncaught Error") && message.contains("is readonly")
        ),
        "{:?}",
        on.status
    );
    let counters = on.counters.expect("on counters");
    assert!(
        counters.property_assign_ic_readonly_exits > 0,
        "{counters:?}"
    );
    assert!(
        counters
            .property_assign_ic_fallback_reasons
            .contains_key("readonly_property"),
        "{counters:?}"
    );
}

#[test]
fn property_fetch_profiles_declared_monomorphic_property() {
    let source = "<?php class PerfPropertyProfileHot { public $value = 3; } $object = new PerfPropertyProfileHot(); $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + $object->value; } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"36");
    let counters = result.counters.expect("counters");
    let profile = property_fetch_profile(&counters, "value");
    assert_eq!(profile.receiver_classes.len(), 1, "{profile:?}");
    assert!(profile.saw_declared_visible_property, "{profile:?}");
    assert_eq!(
        profile
            .property_slot_indexes
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert!(profile.non_eligible_reasons.is_empty(), "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"state\": \"monomorphic\""), "{json}");
    assert!(json.contains("\"fast_path_eligible\": true"), "{json}");
}

#[test]
fn property_fetch_inline_cache_handles_private_property_within_class() {
    let source = "<?php class PerfPropertyPrivate { private $value = 4; public function read() { return $this->value; } } $object = new PerfPropertyPrivate(); for ($i = 0; $i < 8; $i++) { echo $object->read(); }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"44444444");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_ic_hits > 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.property_ic_guard_failures, 0);
}

#[test]
fn property_fetch_inline_cache_handles_inherited_property() {
    let source = "<?php class PerfPropertyBase { public $value = 'B'; } class PerfPropertyChild extends PerfPropertyBase {} $object = new PerfPropertyChild(); for ($i = 0; $i < 8; $i++) { echo $object->value; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"BBBBBBBB");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_ic_hits > 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.property_ic_guard_failures, 0);
}

#[test]
fn property_fetch_inline_cache_guard_fails_for_alternating_receivers() {
    let source = "<?php class PerfPropertyA { public $value = 'A'; } class PerfPropertyB { public $value = 'B'; } $flip = false; for ($i = 0; $i < 8; $i++) { if ($flip) { $object = new PerfPropertyB(); $flip = false; } else { $object = new PerfPropertyA(); $flip = true; } echo $object->value; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"ABABABAB");
    let counters = on.counters.expect("on counters");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
    assert!(counters.property_ic_guard_failures > 0, "{counters:?}");
    assert!(
        counters.optimized_exit_snapshots_created >= counters.property_ic_guard_failures,
        "{counters:?}"
    );
    assert_eq!(
        counters.optimized_exit_snapshots_created,
        counters.optimized_exit_snapshots_materialized
    );
    assert!(
        counters.fallback_resume_successes >= counters.property_ic_guard_failures,
        "{counters:?}"
    );
}

#[test]
fn property_fetch_profiles_polymorphic_and_megamorphic_receivers() {
    let polymorphic = execute_source_with_options(
        "<?php class PerfProfilePolyA { public $value = 'A'; } class PerfProfilePolyB { public $value = 'B'; } function perf_profile_read($object) { return $object->value; } echo perf_profile_read(new PerfProfilePolyA()), perf_profile_read(new PerfProfilePolyB());",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(polymorphic.status.is_success(), "{:?}", polymorphic.status);
    assert_eq!(polymorphic.output.as_bytes(), b"AB");
    let counters = polymorphic.counters.expect("polymorphic counters");
    let profile = property_fetch_profile(&counters, "value");
    assert_eq!(profile.receiver_classes.len(), 2, "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"state\": \"polymorphic\""), "{json}");
    assert!(json.contains("\"polymorphic_receiver\""), "{json}");

    let megamorphic = execute_source_with_options(
        "<?php class PerfProfileMegaA { public $value = 'A'; } class PerfProfileMegaB { public $value = 'B'; } class PerfProfileMegaC { public $value = 'C'; } class PerfProfileMegaD { public $value = 'D'; } class PerfProfileMegaE { public $value = 'E'; } function perf_profile_mega_read($object) { return $object->value; } echo perf_profile_mega_read(new PerfProfileMegaA()), perf_profile_mega_read(new PerfProfileMegaB()), perf_profile_mega_read(new PerfProfileMegaC()), perf_profile_mega_read(new PerfProfileMegaD()), perf_profile_mega_read(new PerfProfileMegaE());",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(megamorphic.status.is_success(), "{:?}", megamorphic.status);
    assert_eq!(megamorphic.output.as_bytes(), b"ABCDE");
    let counters = megamorphic.counters.expect("megamorphic counters");
    let profile = property_fetch_profile(&counters, "value");
    assert_eq!(profile.receiver_classes.len(), 5, "{profile:?}");
    let json = counters.to_json();
    assert!(json.contains("\"state\": \"megamorphic\""), "{json}");
    assert!(json.contains("\"megamorphic_receiver\""), "{json}");
}

#[test]
fn property_fetch_inline_cache_preserves_dynamic_property_fallback() {
    let source = "<?php class PerfDynamicProperty {} $object = new PerfDynamicProperty(); $object->value = 'd'; for ($i = 0; $i < 6; $i++) { echo $object->value; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"dddddd");
    let counters = on.counters.expect("on counters");
    assert_eq!(counters.property_ic_hits, 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
}

#[test]
fn property_fetch_profiles_dynamic_property_fallback() {
    let result = execute_source_with_options(
        "<?php class PerfProfileDynamicProperty {} $object = new PerfProfileDynamicProperty(); $object->value = 'd'; echo $object->value;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"d");
    let counters = result.counters.expect("counters");
    let profile = property_fetch_profile(&counters, "value");
    assert!(profile.dynamic_property_fallback, "{profile:?}");
    assert!(
        profile
            .non_eligible_reasons
            .contains("dynamic_property_fallback"),
        "{profile:?}"
    );
    assert!(
        profile
            .non_eligible_reasons
            .contains("missing_declared_property"),
        "{profile:?}"
    );
}

#[test]
fn property_fetch_inline_cache_preserves_magic_get_fallback() {
    let source = "<?php class PerfMagicProperty { public function __get($name) { return $name . '!'; } } $object = new PerfMagicProperty(); for ($i = 0; $i < 4; $i++) { echo $object->missing; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"missing!missing!missing!missing!");
    let counters = on.counters.expect("on counters");
    assert_eq!(counters.property_ic_hits, 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
}

#[test]
fn property_fetch_profiles_magic_get_reason() {
    let result = execute_source_with_options(
        "<?php class PerfProfileMagicProperty { public function __get($name) { return $name . '!'; } } $object = new PerfProfileMagicProperty(); echo $object->missing;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"missing!");
    let counters = result.counters.expect("counters");
    let profile = property_fetch_profile(&counters, "missing");
    assert!(profile.has_magic_get, "{profile:?}");
    assert!(
        profile.non_eligible_reasons.contains("magic_get_present"),
        "{profile:?}"
    );
    assert!(
        profile
            .non_eligible_reasons
            .contains("missing_declared_property"),
        "{profile:?}"
    );
}

#[test]
fn magic_dispatch_recursion_guards_return_deterministic_errors() {
    let isset = execute_source(
        "<?php class RecursiveMagicIsset { public function __isset($name) { echo $name; return isset($this->$name); } } $object = new RecursiveMagicIsset(); var_export(isset($object->missing));",
    );

    assert!(isset.status.is_success(), "{:?}", isset.status);
    assert_eq!(isset.output.as_bytes(), b"missingfalse");

    let property = execute_source(
        "<?php class RecursiveMagicProperty { public function __get($name) { return $this->$name; } } echo (new RecursiveMagicProperty())->missing;",
    );

    assert!(!property.status.is_success(), "{:?}", property.status);
    assert!(
        property
            .status
            .message()
            .is_some_and(|message| message.contains("E_PHP_VM_MAGIC_PROPERTY_RECURSION")),
        "{:?}",
        property.status
    );

    let method = execute_source(
        "<?php class RecursiveMagicMethod { public function __call($name, $args) { return $this->$name(); } } echo (new RecursiveMagicMethod())->missing();",
    );

    assert!(!method.status.is_success(), "{:?}", method.status);
    assert!(
        method
            .status
            .message()
            .is_some_and(|message| message.contains("E_PHP_VM_MAGIC_METHOD_RECURSION")),
        "{:?}",
        method.status
    );
}

#[test]
fn property_fetch_inline_cache_preserves_property_hook_fallback() {
    let source = "<?php class PerfHookProperty { public string $name { get { return 'hook'; } } } $object = new PerfHookProperty(); for ($i = 0; $i < 4; $i++) { echo $object->name; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"hookhookhookhook");
    let counters = on.counters.expect("on counters");
    assert_eq!(counters.property_ic_hits, 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
}

#[test]
fn property_fetch_profiles_property_hook_reason() {
    let result = execute_source_with_options(
        "<?php class PerfProfileHookProperty { public string $name { get { return 'hook'; } } } $object = new PerfProfileHookProperty(); echo $object->name;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"hook");
    let counters = result.counters.expect("counters");
    let profile = property_fetch_profile(&counters, "name");
    assert!(profile.has_property_hook, "{profile:?}");
    assert!(profile.saw_declared_visible_property, "{profile:?}");
    assert!(
        profile
            .non_eligible_reasons
            .contains("property_hook_present"),
        "{profile:?}"
    );
}

#[test]
fn property_fetch_inline_cache_preserves_uninitialized_typed_property_error() {
    let result = execute_source_with_options(
        "<?php class PerfTypedProperty { public int $value; } $object = new PerfTypedProperty(); echo $object->value;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
            result
                .output
                .to_string_lossy()
                .contains("Uncaught Error: Typed property PerfTypedProperty::$value must not be accessed before initialization"),
            "{}",
            result.output.to_string_lossy()
        );
    let counters = result.counters.expect("counters");
    assert_eq!(counters.property_ic_hits, 0, "{counters:?}");
    assert!(counters.property_ic_misses > 0, "{counters:?}");
}

#[test]
fn property_fetch_profiles_uninitialized_typed_property_reason() {
    let result = execute_source_with_options(
        "<?php class PerfProfileTypedProperty { public int $value; } $object = new PerfProfileTypedProperty(); echo $object->value;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let counters = result.counters.expect("counters");
    let profile = property_fetch_profile(&counters, "value");
    assert!(profile.saw_uninitialized_typed_property, "{profile:?}");
    assert!(
        profile
            .non_eligible_reasons
            .contains("uninitialized_typed_property"),
        "{profile:?}"
    );
    assert!(profile.saw_declared_visible_property, "{profile:?}");
}

#[test]
fn class_static_inline_cache_records_repeated_class_constant_hits() {
    let source = "<?php class PerfConstHot { public const VALUE = 5; } $sum = 0; for ($i = 0; $i < 12; $i++) { $sum = $sum + PerfConstHot::VALUE; } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"60");
    let counters = on.counters.expect("on counters");
    assert!(counters.class_static_ic_hits > 0, "{counters:?}");
    assert!(counters.class_static_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.class_static_ic_guard_failures, 0);
}

#[test]
fn class_static_inline_cache_handles_inherited_constant() {
    let source = "<?php class PerfConstBase { public const VALUE = 'B'; } class PerfConstChild extends PerfConstBase {} for ($i = 0; $i < 8; $i++) { echo PerfConstChild::VALUE; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"BBBBBBBB");
    let counters = on.counters.expect("on counters");
    assert!(counters.class_static_ic_hits > 0, "{counters:?}");
    assert!(counters.class_static_ic_misses > 0, "{counters:?}");
}

#[test]
fn class_static_inline_cache_handles_enum_case_access() {
    let source = "<?php enum PerfCacheStatus: string { case Ready = 'ready'; } for ($i = 0; $i < 6; $i++) { echo PerfCacheStatus::Ready->value; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"readyreadyreadyreadyreadyready");
    let counters = on.counters.expect("on counters");
    assert!(counters.class_static_ic_hits > 0, "{counters:?}");
    assert!(counters.class_static_ic_misses > 0, "{counters:?}");
}

#[test]
fn class_static_inline_cache_reads_static_property_metadata_without_stale_value() {
    let source = "<?php class PerfStaticCache { public static $value = 1; } echo PerfStaticCache::$value; PerfStaticCache::$value = 7; for ($i = 0; $i < 6; $i++) { echo PerfStaticCache::$value; }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"1777777");
    let counters = on.counters.expect("on counters");
    assert!(counters.class_static_ic_hits > 0, "{counters:?}");
    assert!(counters.class_static_ic_misses > 0, "{counters:?}");
    assert_eq!(counters.class_static_ic_guard_failures, 0);
}

#[test]
fn class_static_inline_cache_guards_late_static_binding() {
    let source = "<?php class PerfLsbBase { public const VALUE = 'A'; public static function read() { return static::VALUE; } } class PerfLsbChild extends PerfLsbBase { public const VALUE = 'B'; } $flip = false; for ($i = 0; $i < 8; $i++) { if ($flip) { echo PerfLsbChild::read(); $flip = false; } else { echo PerfLsbBase::read(); $flip = true; } }";
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output.as_bytes(), b"ABABABAB");
    let counters = on.counters.expect("on counters");
    assert!(counters.class_static_ic_misses > 0, "{counters:?}");
    assert!(counters.class_static_ic_guard_failures > 0, "{counters:?}");
}

#[test]
fn inline_cache_type_changes_reach_polymorphic_and_megamorphic_without_output_changes() {
    let source = "<?php class PerfProtoA { public $prop = 'A'; public function value() { return 'A'; } } class PerfProtoB { public $prop = 'B'; public function value() { return 'B'; } } class PerfProtoC { public $prop = 'C'; public function value() { return 'C'; } } class PerfProtoD { public $prop = 'D'; public function value() { return 'D'; } } class PerfProtoE { public $prop = 'E'; public function value() { return 'E'; } } function perf_emit_proto($object) { echo $object->value(), $object->prop; } $a = new PerfProtoA(); $b = new PerfProtoB(); $c = new PerfProtoC(); $d = new PerfProtoD(); $e = new PerfProtoE(); perf_emit_proto($a); perf_emit_proto($b); perf_emit_proto($a); perf_emit_proto($b); perf_emit_proto($c); perf_emit_proto($d); perf_emit_proto($e); perf_emit_proto($a);";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("on counters");
    assert!(counters.method_ic_hits > 0, "{counters:?}");
    assert!(counters.property_ic_hits > 0, "{counters:?}");
    assert!(counters.inline_cache_polymorphic > 0, "{counters:?}");
    assert!(counters.inline_cache_megamorphic >= 2, "{counters:?}");
    assert_eq!(counters.inline_cache_disabled, 0, "{counters:?}");
}

#[test]
fn quickening_type_changes_dequicken_to_megamorphic_without_output_changes() {
    let source = "<?php $a = 1; $b = 1; $last = 0; for ($i = 0; $i < 16; $i++) { if ($i < 10) { $b = 1; } else if ($b === 1) { $b = '2'; } else { $b = 1.5; } $last = $a + $b; } echo $last;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("on counters");
    assert!(counters.quickening_guard_misses >= 2, "{counters:?}");
    assert!(counters.quickening_guard_failures >= 2, "{counters:?}");
    assert!(counters.quickening_fallback_calls >= 2, "{counters:?}");
    assert!(counters.quickening_dequickens > 0, "{counters:?}");
    assert!(counters.quickening_megamorphic > 0, "{counters:?}");
    assert!(
        counters.optimized_exit_snapshots_created >= counters.quickening_guard_failures,
        "{counters:?}"
    );
    assert_eq!(
        counters.optimized_exit_snapshots_created,
        counters.optimized_exit_snapshots_materialized
    );
    assert!(
        counters.fallback_resume_successes >= counters.quickening_guard_failures,
        "{counters:?}"
    );
}

#[test]
fn add_int_int_quickening_records_guard_hits_for_hot_loop() {
    let source =
        "<?php $sum = 0; for ($i = 0; $i < 20; $i++) { $sum = $sum + $i; } echo $sum, \"\\n\";";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    let on_counters = on.counters.expect("on counters");
    assert!(on_counters.quickening_guard_hits > 0, "{on_counters:?}");
    assert_eq!(on_counters.quickening_guard_misses, 0);
}

#[test]
fn add_int_int_quickening_falls_back_on_overflow() {
    let source = "<?php $a = 1; $b = 1; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $a = 9223372036854775807; } $sum = $a + $b; } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert_eq!(on.status, off.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    assert!(
        on.status.is_success(),
        "overflow should use generic promoted result"
    );
    let on_counters = on.counters.expect("on counters");
    assert!(on_counters.quickening_guard_misses > 0, "{on_counters:?}");
}

#[test]
fn add_int_int_quickening_falls_back_on_float_and_numeric_string() {
    let cases = [
        (
            "float",
            "<?php $a = 1; $b = 1; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $b = 1.5; } $sum = $a + $b; } echo $sum;",
        ),
        (
            "numeric string",
            "<?php $a = 1; $b = 1; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $b = \"2\"; } $sum = $a + $b; } echo $sum;",
        ),
    ];

    for (name, source) in cases {
        let off = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                quickening: QuickeningMode::Off,
                ..VmOptions::default()
            },
        );
        let on = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                quickening: QuickeningMode::On,
                ..VmOptions::default()
            },
        );

        assert!(off.status.is_success(), "{name}: {:?}", off.status);
        assert!(on.status.is_success(), "{name}: {:?}", on.status);
        assert_eq!(on.output, off.output, "{name}");
        assert_eq!(on.diagnostics, off.diagnostics, "{name}");
        let on_counters = on.counters.expect("on counters");
        assert!(
            on_counters.quickening_guard_misses > 0,
            "{name}: {on_counters:?}"
        );
    }
}

#[test]
fn add_int_int_quickening_preserves_reference_cow_behavior() {
    let source = "<?php $a = 1; $b = 1; $r =& $b; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $r = 4; } $sum = $a + $b; } echo $sum, ':', $r;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let on_counters = on.counters.expect("on counters");
    assert!(on_counters.quickening_guard_hits > 0, "{on_counters:?}");
}

#[test]
fn concat_string_string_quickening_records_hits_for_hot_append_loop() {
    let source = "<?php $s = ''; for ($i = 0; $i < 20; $i++) { $s .= 'x'; } echo $s;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters.string_concat_fast_path_hits > 0,
        "{on_counters:?}"
    );
    assert_eq!(on_counters.string_concat_fast_path_misses, 0);
}

#[test]
fn concat_string_string_quickening_records_hits_for_binary_concat() {
    let source =
        "<?php $a = 'left'; $b = 'right'; for ($i = 0; $i < 16; $i++) { $s = $a . $b; } echo $s;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters.string_concat_fast_path_hits > 0,
        "{on_counters:?}"
    );
}

#[test]
fn concat_string_string_quickening_falls_back_for_object_and_int_conversion() {
    let cases = [
        (
            "object",
            "<?php class QS { public function __toString(): string { return 'object'; } } $a = 'a'; $b = 'b'; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $a = new QS(); } $s = $a . $b; } echo $s;",
        ),
        (
            "int",
            "<?php $a = 'a'; $b = 'b'; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $a = 7; } $s = $a . $b; } echo $s;",
        ),
    ];

    for (name, source) in cases {
        let off = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                quickening: QuickeningMode::Off,
                ..VmOptions::default()
            },
        );
        let on = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                quickening: QuickeningMode::On,
                ..VmOptions::default()
            },
        );

        assert!(off.status.is_success(), "{name}: {:?}", off.status);
        assert!(on.status.is_success(), "{name}: {:?}", on.status);
        assert_eq!(on.output, off.output, "{name}");
        assert_eq!(on.diagnostics, off.diagnostics, "{name}");
        let on_counters = on.counters.expect("on counters");
        assert!(
            on_counters.string_concat_fast_path_misses > 0,
            "{name}: {on_counters:?}"
        );
    }
}

#[test]
fn concat_string_string_quickening_preserves_string_cow_and_references() {
    let source = "<?php $a = 'a'; $alias =& $a; $copy = $a; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $alias = 'z'; } $s = $a . 'x'; } echo $s, '|', $copy, '|', $alias;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters.string_concat_fast_path_hits > 0,
        "{on_counters:?}"
    );
}

#[test]
fn packed_dim_quickening_records_hits_for_hot_list_fetch_loop() {
    let source = "<?php $items = [1,2,3]; $sum = 0; for ($i = 0; $i < 12; $i++) { $sum += $items[1]; } echo $sum;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let on_counters = on.counters.expect("on counters");
    assert!(on_counters.packed_dim_fast_path_hits > 0, "{on_counters:?}");
    assert_eq!(on_counters.packed_dim_fast_path_misses, 0);
}

#[test]
fn packed_dim_quickening_falls_back_for_oob_with_warning_and_null() {
    let source = "<?php $items = [10,20,30]; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $items = []; } $value = $items[1]; } echo $value;";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    assert!(
        on.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING")
    );
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters.packed_dim_fast_path_misses > 0,
        "{on_counters:?}"
    );
}

#[test]
fn dense_auto_fetch_dim_keeps_unrendered_warning_structured() {
    let source = "<?php $items = []; echo $items['missing']; echo \"x\\n\";";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"x\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING"),
        "{:?}",
        result.diagnostics
    );
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.bytecode_instructions_executed > 0, "{counters:?}");
}

#[test]
fn scalar_fetch_dim_warns_and_returns_null_in_ir() {
    assert_scalar_fetch_dim_warns_and_returns_null(ExecutionFormat::Ir);
}

#[test]
fn scalar_fetch_dim_warns_and_returns_null_in_auto_bytecode() {
    assert_scalar_fetch_dim_warns_and_returns_null(ExecutionFormat::Auto);
}

fn assert_scalar_fetch_dim_warns_and_returns_null(execution_format: ExecutionFormat) {
    let source = r#"<?php
            foreach ([null, false, true, 1, 1.2] as $value) {
                echo $value['name'] === null ? '|n|' : '|x|';
            }
        "#;
    let result = execute_source_with_options(
        source,
        VmOptions {
            execution_format,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy().matches("|n|").count(), 5);
    assert!(!result.output.to_string_lossy().contains("|x|"));
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.id() == "E_PHP_RUNTIME_ARRAY_OFFSET_ON_SCALAR_WARNING"
            })
            .count(),
        5,
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn dense_mixed_array_fetch_reuses_borrowed_receiver() {
    let source = "<?php
            $row = ['score' => 7, 'name' => 'x'];
            $sum = 0;
            for ($i = 0; $i < 200; $i++) {
                $sum += $row['score'];
            }
            echo $sum;
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1400");
    assert_eq!(result.diagnostics, Vec::<RuntimeDiagnostic>::new());
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.bytecode_instructions_executed > 0, "{counters:?}");
    assert!(
        counters.array_handle_clones <= 300,
        "mixed array reads should not clone the receiver per fetch: {counters:?}"
    );
}

#[test]
fn packed_dim_quickening_falls_back_for_mixed_array_and_numeric_string_key() {
    let mixed_source = "<?php $items = [10,20,30]; for ($i = 0; $i < 12; $i++) { if ($i === 10) { $items[5] = 99; } $value = $items[1]; } echo $value;";
    let off = execute_source_with_options(
        mixed_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        mixed_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters.packed_dim_fast_path_misses > 0,
        "{on_counters:?}"
    );

    let numeric_string_source = "<?php $items = [10,20,30]; echo $items[1], '|', $items['1'];";
    let off = execute_source_with_options(
        numeric_string_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        numeric_string_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.diagnostics, off.diagnostics);
    assert_eq!(on.output.as_bytes(), b"20|20");
}

#[test]
fn packed_dim_quickening_does_not_specialize_by_ref_element_access() {
    let source = "<?php $items = [1,2,3]; for ($i = 0; $i < 12; $i++) { $r =& $items[1]; $r = $r + 1; } echo $items[1];";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"14");
    let counters = result.counters.expect("counters");
    assert_eq!(counters.packed_dim_fast_path_hits, 0, "{counters:?}");
    assert_eq!(counters.packed_dim_fast_path_misses, 0, "{counters:?}");
}

#[test]
fn array_fast_paths_record_packed_append_read_foreach_and_count_hits() {
    let source = "<?php
            $items = [];
            for ($i = 0; $i < 8; $i++) {
                $items[] = $i;
            }
            $sum = 0;
            foreach ($items as $value) {
                $sum += $value;
            }
            $read = 0;
            for ($j = 0; $j < 12; $j++) {
                $read += $items[3];
            }
            echo $sum, '|', count($items), '|', $read;
        ";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(on.output.as_bytes(), b"28|8|36");
    assert_eq!(on.diagnostics, off.diagnostics);
    let counters = on.counters.expect("counters");
    assert!(
        counters.array_packed_append_fast_path_hits >= 7,
        "{counters:?}"
    );
    assert!(counters.packed_append_fast_hits >= 7, "{counters:?}");
    assert!(
        counters.array_packed_read_fast_path_hits > 0,
        "{counters:?}"
    );
    assert!(counters.packed_fetch_fast_hits > 0, "{counters:?}");
    assert!(
        counters.array_sequential_foreach_fast_path_hits > 0,
        "{counters:?}"
    );
    assert!(counters.packed_foreach_fast_hits > 0, "{counters:?}");
    assert!(counters.array_count_fast_path_hits > 0, "{counters:?}");
    assert_eq!(counters.array_packed_to_mixed_transitions, 0);
}

#[test]
fn packed_array_fast_paths_record_guard_fallback_reasons() {
    let bounds_source = "<?php
            $items = [10, 20, 30];
            $sum = 0;
            for ($i = 0; $i < 16; $i++) {
                if ($i === 12) {
                    unset($items[2]);
                }
                $sum += $items[2];
            }
            echo $sum;
        ";
    let bounds = execute_source_with_options(
        bounds_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );
    assert!(bounds.status.is_success(), "{:?}", bounds.status);
    let bounds_counters = bounds.counters.expect("bounds counters");
    assert!(
        bounds_counters.packed_fetch_bounds_fallbacks > 0,
        "{bounds_counters:?}"
    );

    let layout_source = "<?php
            $items = [10, 20, 30];
            $sum = 0;
            for ($i = 0; $i < 16; $i++) {
                if ($i === 12) {
                    $items['x'] = 40;
                }
                $sum += $items[1];
            }
            echo $sum;
        ";
    let layout = execute_source_with_options(
        layout_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );
    assert!(layout.status.is_success(), "{:?}", layout.status);
    let layout_counters = layout.counters.expect("layout counters");
    assert!(
        layout_counters.packed_fetch_layout_fallbacks > 0,
        "{layout_counters:?}"
    );

    let cow_reference_source = "<?php
            $items = [1, 2, 3];
            $copy = $items;
            $copy[] = 4;
            $ref =& $items[1];
            foreach ($items as $value) {
                echo $value;
            }
        ";
    let cow_reference = execute_source_with_options(
        cow_reference_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );
    assert!(
        cow_reference.status.is_success(),
        "{:?}",
        cow_reference.status
    );
    let cow_reference_counters = cow_reference.counters.expect("cow counters");
    assert!(
        cow_reference_counters.cow_or_reference_fallbacks > 0,
        "{cow_reference_counters:?}"
    );
}

#[test]
fn array_shape_observers_record_record_small_map_and_fallbacks() {
    let record_source = "<?php
            $route = ['id' => 42, 'slug' => 'post'];
            $config = ['name' => 'app', 'env' => 'test'];
            $json = ['id' => 7, 'name' => 'Ada'];
            $row = ['id' => 3, 'email' => 'a@example.test'];
            echo isset($route['id']) ? $route['id'] : 0;
            echo '|', $config['name'], '|', $json['name'], '|', $row['email'];
            echo '|', isset($route['missing']) ? 'yes' : 'no';
        ";
    let record = execute_source_with_options(
        record_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(record.status.is_success(), "{:?}", record.status);
    assert_eq!(record.output.as_bytes(), b"42|app|Ada|a@example.test|no");
    let counters = record.counters.expect("record counters");
    let record_observations = counters
        .array_shape_observed_by_kind
        .get("shape_stable_record_like")
        .copied()
        .unwrap_or_default()
        + counters
            .array_shape_observed_by_kind
            .get("interned_string_key_record")
            .copied()
            .unwrap_or_default();
    assert!(record_observations > 0, "{counters:?}");
    assert!(counters.record_shape_hits >= 1, "{counters:?}");
    assert!(counters.record_shape_misses >= 1, "{counters:?}");

    let small_map_source = "<?php
            $mixed = [1 => 'one', 'name' => 'two'];
            echo $mixed['name'], '|', isset($mixed[1]) ? $mixed[1] : 'missing';
        ";
    let small = execute_source_with_options(
        small_map_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(small.status.is_success(), "{:?}", small.status);
    assert_eq!(small.output.as_bytes(), b"two|one");
    let counters = small.counters.expect("small counters");
    assert!(
        counters
            .array_shape_observed_by_kind
            .get("small_inline_map")
            .copied()
            .unwrap_or_default()
            > 0,
        "{counters:?}"
    );
    assert!(counters.small_map_hits >= 1, "{counters:?}");

    let fallback_source = "<?php
            $numeric = ['id' => 'leading'];
            echo isset($numeric[0]) ? 'yes' : 'no';
            $order = [0 => 'a', 2 => 'b'];
            echo '|', isset($order[2]) ? 'yes' : 'no';
            $unset = ['id' => 1, 'name' => 2];
            unset($unset['id']);
            $unset['id'] = 3;
            echo '|', $unset['id'];
            $cow = ['id' => 4];
            $copy = $cow;
            echo '|', $copy['id'];
            $ref = ['id' => 5];
            $alias =& $ref['id'];
            echo '|', isset($ref['id']) ? 'yes' : 'no';
        ";
    let fallback = execute_source_with_options(
        fallback_source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(fallback.status.is_success(), "{:?}", fallback.status);
    assert_eq!(fallback.output.as_bytes(), b"no|yes|3|4|yes");
    let counters = fallback.counters.expect("fallback counters");
    assert!(counters.key_coercion_fallbacks >= 1, "{counters:?}");
    assert!(counters.order_semantics_fallbacks >= 1, "{counters:?}");
    assert!(counters.cow_or_reference_fallbacks >= 1, "{counters:?}");
    assert!(
        counters
            .array_shape_observed_by_kind
            .get("cow_or_reference_fallback")
            .copied()
            .unwrap_or_default()
            > 0,
        "{counters:?}"
    );
}

#[test]
fn array_fast_paths_record_packed_to_mixed_transitions() {
    let source = "<?php
            $nonseq = [1, 2];
            $nonseq[5] = 5;
            echo $nonseq[1], '|';

            $stringKey = [1, 2];
            $stringKey['x'] = 3;
            echo count($stringKey), '|';

            $hole = [1, 2, 3];
            unset($hole[1]);
            $hole[] = 4;
            foreach ($hole as $key => $value) {
                echo $key, ':', $value, ',';
            }
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "2|3|0:1,2:3,3:4,");
    let counters = result.counters.expect("counters");
    assert!(
        counters.array_packed_to_mixed_transitions >= 3,
        "{counters:?}"
    );
}

#[test]
fn array_fast_paths_preserve_references_and_foreach_mutation_order() {
    let source = "<?php
            $items = [1];
            $ref =& $items[0];
            $items[] = 2;
            $ref = 9;
            foreach ($items as $key => $value) {
                echo $key, ':', $value, ',';
                if ($key === 0) {
                    $items[] = 3;
                }
            }
            echo '|', $items[0], '|', $items[2], '|', count($items);
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "0:9,1:2,|9|3|3");
    let counters = result.counters.expect("counters");
    assert!(
        counters.array_packed_append_fast_path_hits > 0,
        "{counters:?}"
    );
    assert!(counters.packed_append_fast_hits > 0, "{counters:?}");
    assert!(counters.cow_or_reference_fallbacks > 0, "{counters:?}");
}

#[test]
fn numeric_string_cache_records_hits_for_hot_arithmetic_comparison_and_casts() {
    let source = "<?php
            $s = \" 42\\t\";
            $sum = 0;
            for ($i = 0; $i < 12; $i++) {
                $sum += $s;
                if ($s == 42) {
                    $sum += 1;
                }
                $last = (int) $s;
            }
            $big = \"9223372036854775808\";
            $large = 0;
            for ($j = 0; $j < 4; $j++) {
                if ($big > 1) {
                    $large++;
                }
            }
            echo $sum, '|', $last, '|', $large;
        ";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"516|42|4");
    let counters = result.counters.expect("counters");
    assert!(counters.numeric_string_classify_calls > 0, "{counters:?}");
    assert!(counters.numeric_string_cache_hits > 0, "{counters:?}");
    assert!(counters.numeric_string_cache_misses > 0, "{counters:?}");
    assert!(
        counters.numeric_string_specialization_hits > 0,
        "{counters:?}"
    );
    assert!(
        counters.numeric_string_overflow_precision_fallbacks > 0,
        "{counters:?}"
    );
    assert!(
        counters.numeric_string_cache_hits > counters.numeric_string_cache_misses,
        "{counters:?}"
    );
}

#[test]
fn numeric_string_cache_does_not_cache_or_delay_non_numeric_diagnostics() {
    let result = execute_source_with_options(
        "<?php $s = 'abc'; echo $s + 1;",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.status.message(),
        Some("E_PHP_RUNTIME_NON_NUMERIC_STRING: Unsupported operand types: string + int")
    );
    let counters = result.counters.expect("counters");
    assert_eq!(counters.numeric_string_classify_calls, 1, "{counters:?}");
    assert_eq!(counters.numeric_string_cache_misses, 1, "{counters:?}");
    assert_eq!(counters.numeric_string_cache_hits, 0, "{counters:?}");
    assert_eq!(
        counters.numeric_string_specialization_hits, 0,
        "{counters:?}"
    );
}

#[test]
fn local_slot_fast_path_records_hits_for_simple_hot_loop() {
    let source = "<?php $sum = 0; for ($i = 0; $i < 20; $i++) { $sum = $sum + $i; } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"190");
    let counters = result.counters.expect("counters");
    assert!(counters.local_slot_fast_path_hits > 0, "{counters:?}");
    assert_eq!(counters.local_slot_fast_path_misses, 0, "{counters:?}");
}

#[test]
fn local_slot_fast_path_counts_global_symbol_table_fallback() {
    let source =
        "<?php $g = 7; function bump_global() { global $g; $g = $g + 1; } bump_global(); echo $g;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"8");
    let counters = result.counters.expect("counters");
    assert!(counters.local_slot_fast_path_hits > 0, "{counters:?}");
    assert!(counters.local_slot_fast_path_misses > 0, "{counters:?}");
}

#[test]
fn local_slot_fast_path_preserves_by_ref_params() {
    let source =
        "<?php function bump_ref(&$x) { $x = $x + 1; } $value = 1; bump_ref($value); echo $value;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2");
    let counters = result.counters.expect("counters");
    assert!(counters.local_slot_fast_path_hits > 0, "{counters:?}");
}

#[test]
fn internal_builtin_by_ref_metadata_uses_generated_arginfo() {
    assert!(internal_builtin_param_requires_reference("apcu_inc", 2));
    assert!(internal_builtin_param_requires_reference("apcu_dec", 2));
    assert_eq!(internal_builtin_by_ref_param_name("apcu_inc", 2), "success");
    assert!(internal_builtin_param_requires_reference("pcntl_wait", 0));
    assert!(internal_builtin_param_requires_reference(
        "pcntl_waitpid",
        1
    ));
    assert!(internal_builtin_param_requires_reference(
        "pcntl_waitpid",
        3
    ));
    assert_eq!(
        internal_builtin_by_ref_param_name("pcntl_waitpid", 1),
        "status"
    );
    assert_eq!(
        internal_builtin_by_ref_param_name("pcntl_waitpid", 3),
        "resource_usage"
    );
    assert!(internal_builtin_param_requires_reference(
        "openssl_random_pseudo_bytes",
        1
    ));
    assert_eq!(
        internal_builtin_by_ref_param_name("openssl_random_pseudo_bytes", 1),
        "strong_result"
    );
    assert!(internal_builtin_param_requires_reference(
        "openssl_encrypt",
        5
    ));
    assert_eq!(
        internal_builtin_by_ref_param_name("openssl_encrypt", 5),
        "tag"
    );
    assert!(!internal_builtin_param_requires_reference(
        "pcntl_waitpid",
        2
    ));
}

#[test]
fn internal_builtin_generated_by_ref_out_param_suppresses_undefined_warning() {
    let result = execute_source(
        r#"<?php
$iv = str_repeat("0", openssl_cipher_iv_length("aes-128-cbc"));
openssl_encrypt("payload", "aes-128-cbc", "secret", 0, $iv, $tag);
var_dump($tag);
"#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"NULL\n");
}

#[test]
fn local_slot_fast_path_preserves_closure_use_slots() {
    let source =
        "<?php $base = 3; $f = function ($x) use ($base) { return $x + $base; }; echo $f(4);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
    let counters = result.counters.expect("counters");
    assert!(counters.local_slot_fast_path_hits > 0, "{counters:?}");
}

#[test]
fn local_slot_fast_path_preserves_global_and_superglobal_fallbacks() {
    let source = "<?php $value = 5; echo $GLOBALS['value'], '|', isset($_SERVER['argv']);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5|1");
    let counters = result.counters.expect("counters");
    assert!(counters.local_slot_fast_path_hits > 0, "{counters:?}");
    assert!(counters.local_slot_fast_path_misses > 0, "{counters:?}");
}

#[test]
fn frame_reuse_records_reuse_for_call_heavy_loop() {
    let source = "<?php function inc_frame_reuse($x) { return $x + 1; } $sum = 0; for ($i = 0; $i < 20; $i++) { $sum = inc_frame_reuse($sum); } echo $sum;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"20");
    let counters = result.counters.expect("counters");
    assert!(counters.frame_allocations > 0, "{counters:?}");
    assert!(counters.frame_reuses > 0, "{counters:?}");
    assert_eq!(counters.frames_allocated, counters.frame_allocations);
    assert_eq!(counters.frames_reused, counters.frame_reuses);
    assert_eq!(
        counters.register_files_allocated,
        counters.frame_allocations
    );
    assert_eq!(counters.register_files_reused, counters.frame_reuses);
    assert_eq!(
        counters.request_arena_allocations,
        counters.frames_allocated
    );
    assert!(counters.request_arena_bytes > 0, "{counters:?}");
    assert_eq!(counters.request_pool_resets, counters.frames_reused);
    // The persistent immutable engine heap (interned names) is now
    // accounted as a footprint; a program that declares and calls
    // functions interns at least those names, so both are non-zero.
    assert!(
        counters.persistent_engine_allocations > 0,
        "persistent engine names should be accounted: {counters:?}"
    );
    assert!(
        counters.persistent_engine_bytes > 0,
        "persistent engine bytes should be accounted: {counters:?}"
    );
}

#[test]
fn specialized_call_frames_record_layouts_and_preserve_fallbacks() {
    let source = r#"<?php
function tiny_frame_add($a, $b) { return $a + $b; }
function call_context_frame($a) { return func_num_args() . ':' . count(func_get_args()); }
class FrameLayoutService { public function inc($x) { return $x + 1; } }
function named_frame($a, $b = 2) { return $a + $b; }
function variadic_frame(...$xs) { return count($xs); }
function byref_frame(&$x) { $x++; }
function gen_frame() { yield 1; }
$sum = 0;
for ($i = 0; $i < 20; $i++) { $sum = tiny_frame_add($sum, 1); }
echo "tiny=$sum\n";
echo "context=", call_context_frame(1, 2), "\n";
$svc = new FrameLayoutService();
for ($i = 0; $i < 3; $i++) { echo "method=", $svc->inc($i), "\n"; }
$base = 3;
$closure = function($x) use ($base) { return $x + $base; };
echo "closure=", $closure(4), "\n";
echo "named=", named_frame(b: 5, a: 4), "\n";
echo "variadic=", variadic_frame(1, 2, 3), "\n";
$value = 1;
byref_frame($value);
echo "byref=$value\n";
$g = gen_frame();
echo "gen=", $g->current(), "\n";
$fiber = new Fiber(function() { Fiber::suspend("fiber"); });
echo "fiber=", $fiber->start(), "\n";
eval('echo "eval=5\n";');
echo "dynamic=", call_user_func('tiny_frame_add', 2, 3), "\n";
"#;
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"tiny=20\ncontext=2:2\nmethod=1\nmethod=2\nmethod=3\nclosure=7\nnamed=9\nvariadic=3\nbyref=2\ngen=1\nfiber=fiber\neval=5\ndynamic=5\n"
        );
    let counters = result.counters.expect("counters");
    for layout in [
        "tiny_leaf_frame",
        "known_function_frame",
        "known_method_frame",
        "closure_frame",
        "variadic_named_argument_frame",
        "generator_frame",
        "fiber_frame",
        "include_eval_frame",
    ] {
        assert!(
            counters
                .call_frame_layout_observed
                .get(layout)
                .is_some_and(|count| *count > 0),
            "missing {layout}: {counters:?}"
        );
    }
    assert!(counters.tiny_frame_candidates > 0, "{counters:?}");
    assert!(counters.specialized_frame_hits > 0, "{counters:?}");
    assert!(counters.arg_array_avoided > 0, "{counters:?}");
    assert!(counters.heap_frame_avoided > 0, "{counters:?}");
    for reason in [
        "not_tiny_leaf",
        "class_context",
        "closure",
        "named_or_variadic",
        "by_ref_param",
        "generator",
        "fiber",
        "include_eval",
    ] {
        assert!(
            counters
                .generic_frame_fallback_by_reason
                .get(reason)
                .is_some_and(|count| *count > 0),
            "missing {reason}: {counters:?}"
        );
    }
}

#[test]
fn frame_reuse_preserves_recursive_calls() {
    let source = "<?php function fact_frame_reuse($n) { if ($n < 2) { return 1; } return $n * fact_frame_reuse($n - 1); } echo fact_frame_reuse(5);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"120");
    let counters = result.counters.expect("counters");
    assert!(counters.frame_allocations >= 5, "{counters:?}");
}

#[test]
fn frame_reuse_blocks_closure_captures_conservatively() {
    let source =
        "<?php $base = 2; $f = function ($x) use ($base) { return $x + $base; }; echo $f(5);";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters
            .frame_reuse_blocked_by_reason
            .get("closure_capture"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn frame_reuse_blocks_by_ref_params_conservatively() {
    let source = "<?php function bump_frame_ref(&$x) { $x = $x + 1; } $value = 1; bump_frame_ref($value); bump_frame_ref($value); echo $value;";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters.frame_reuse_blocked_by_reason.get("by_ref_param"),
        Some(&2),
        "{counters:?}"
    );
}

#[test]
fn frame_reuse_preserves_exceptions_through_calls() {
    let source = "<?php function frame_reuse_finally($v) { try { return $v; } finally { echo 'finally|'; } } echo frame_reuse_finally('ok');";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"finally|ok");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters.frame_reuse_blocked_by_reason.get("try_finally"),
        Some(&1),
        "{counters:?}"
    );
}

#[test]
fn frame_reuse_preserves_destructors_during_unwind() {
    let source = "<?php class FrameReuseDestruct { public function __destruct() { echo 'd'; } } function frame_reuse_make() { $x = new FrameReuseDestruct(); echo 'm|'; } frame_reuse_make(); echo 'after|';";
    let result = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"m|dafter|");
    let counters = result.counters.expect("counters");
    assert_eq!(
        counters
            .frame_reuse_blocked_by_reason
            .get("destructor_sensitive_value"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .arena_fallback_allocations_by_reason
            .get("destructor_sensitive_value"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(counters.destructor_sensitive_arena_blocks, 1);
}

#[test]
fn frame_reuse_preserves_generator_and_fiber_suspension() {
    let generator = execute_source_with_options(
        "<?php function frame_reuse_gen() { yield 'k' => 'v'; } $g = frame_reuse_gen(); echo $g->current();",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(generator.status.is_success(), "{:?}", generator.status);
    assert_eq!(generator.output.as_bytes(), b"v");
    let generator_counters = generator.counters.expect("counters");
    assert!(
        generator_counters
            .frame_reuse_blocked_by_reason
            .get("generator")
            .is_some_and(|count| *count >= 1),
        "{generator_counters:?}"
    );

    let fiber = execute_source_with_options(
        "<?php $fiber = new Fiber(function() { echo 'a'; Fiber::suspend('s'); echo 'b'; }); echo $fiber->start(); echo '|'; $fiber->resume('r');",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(fiber.status.is_success(), "{:?}", fiber.status);
    assert_eq!(fiber.output.as_bytes(), b"as|b");
}

#[test]
fn trace_captures_deterministic_instruction_state() {
    let result = execute_source_with_options(
        "<?php $a = 1; echo $a, \"\\n\";",
        VmOptions {
            trace: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1\n");
    assert!(!result.trace.is_empty());
    assert!(
        result
            .trace
            .iter()
            .all(|line| !line.contains("0x") && !line.contains(" at ")),
        "{:#?}",
        result.trace
    );
    assert!(
        result
            .trace
            .iter()
            .any(|line| line.contains("function=main(0)")
                && line.contains("stack_depth=1")
                && line.contains("output_len=0")),
        "{:#?}",
        result.trace
    );
    assert!(
        result
            .trace
            .iter()
            .any(|line| line.contains("locals=[a=Int(1)]")),
        "{:#?}",
        result.trace
    );
}

#[test]
fn variables_execute_compound_assignment_through_binary_ops() {
    let result = execute_source("<?php $a = 1; $a += 2; $a .= \"x\"; echo $a;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3x");
}

#[test]
fn properties_execute_compound_assignment_through_binary_ops() {
    let result = execute_source(
        "<?php class C { public $s = 'a'; public $n = 1; } $c = new C(); $c->s .= 'b'; $c->n += 2; echo $c->s, '|', $c->n;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ab|3");
}

#[test]
fn variables_execute_integer_braced_names() {
    let result = execute_source("<?php ${10} = 42; var_dump(${10});");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"int(42)\n");
}

#[test]
fn binary_add_executes_php_array_union() {
    let result = execute_source(
        "<?php $a = [0 => 'left', 'k' => 'keep']; $b = [0 => 'right', 1 => 'new', 'k' => 'drop']; var_dump($a + $b);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array(3) {\n  [0]=>\n  string(4) \"left\"\n  [\"k\"]=>\n  string(4) \"keep\"\n  [1]=>\n  string(3) \"new\"\n}\n"
    );
}

#[test]
fn variables_execute_pre_and_post_inc_dec() {
    let result = execute_source("<?php $a = 1; echo $a++, \"|\", ++$a, \"|\", $a--, \"|\", --$a;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|3|3|1");
}

#[test]
fn variables_undefined_fetch_warns_and_reads_null() {
    let result = execute_source("<?php echo $missing, \"x\";");

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Warning: Undefined variable $missing in "));
    assert!(output.contains(" on line "));
    assert!(output.ends_with("x"));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING"
    );
}

#[test]
fn references_execute_by_value_assignment_and_local_alias_mvp() {
    let by_value = execute_source("<?php $a = 1; $b = $a; $b = 2; echo $a, $b;");

    assert!(by_value.status.is_success(), "{:?}", by_value.status);
    assert_eq!(by_value.output.as_bytes(), b"12");

    let alias = execute_source("<?php $a = 1; $b =& $a; $b = 2; echo $a; $a = 3; echo $b;");

    assert!(alias.status.is_success(), "{:?}", alias.status);
    assert_eq!(alias.output.as_bytes(), b"23");

    let undefined_source =
        execute_source("<?php $a = $ref =& $val; var_dump($a); $c =& $missing; echo $c;");

    assert!(
        undefined_source.status.is_success(),
        "{:?}",
        undefined_source.status
    );
    assert_eq!(undefined_source.output.as_bytes(), b"NULL\n");
    assert!(
        undefined_source.diagnostics.is_empty(),
        "{:?}",
        undefined_source.diagnostics
    );
}

#[test]
fn alias_counters_identify_no_reference_hot_path() {
    let result = execute_source_with_options(
        "<?php function sum_plain($n) { $s = 0; for ($i = 0; $i < $n; $i++) { $s = $s + $i; } return $s; } echo sum_plain(12);",
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            quickening: QuickeningMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"66");
    let counters = result.counters.expect("counters");
    assert!(
        counters
            .frame_alias_state
            .get("no_references_observed")
            .is_some_and(|count| *count >= 1),
        "{counters:?}"
    );
    assert_eq!(counters.fast_path_disabled_by_reference, 0, "{counters:?}");
}

#[test]
fn alias_counters_classify_reference_fixture_cases() {
    let cases = [
        (
            "local-only",
            "<?php $a = 1; $b =& $a; $b = 2; echo $a;",
            b"2".as_slice(),
            "local_only_reference",
        ),
        (
            "by-ref-param",
            "<?php function bump_alias(&$x) { $x = $x + 1; } $v = 1; bump_alias($v); echo $v;",
            b"2".as_slice(),
            "escaped_reference",
        ),
        (
            "by-ref-return",
            "<?php function &ret_alias(&$x) { return $x; } $v = 1; $r =& ret_alias($v); $r = 3; echo $v;",
            b"3".as_slice(),
            "escaped_reference",
        ),
        (
            "array-element",
            "<?php $a = []; $v = 1; $a['k'] =& $v; $v = 4; echo $a['k'];",
            b"4".as_slice(),
            "property_or_array_dim_reference",
        ),
        (
            "global",
            "<?php $g = 1; function set_global_alias() { global $g; $g = 5; } set_global_alias(); echo $g;",
            b"5".as_slice(),
            "global_or_superglobal_reference",
        ),
        (
            "unset-rebind",
            "<?php $a = 1; $b =& $a; unset($a); $c = 2; $b =& $c; $b = 6; echo isset($a) ? 'bad' : 'unset', '|', $c;",
            b"unset|6".as_slice(),
            "local_only_reference",
        ),
        (
            "foreach-by-ref",
            "<?php $a = [1, 2]; foreach ($a as &$v) { $v = $v + 1; } unset($v); echo $a[0], $a[1];",
            b"23".as_slice(),
            "property_or_array_dim_reference",
        ),
        (
            "closure-escape",
            "<?php $x = 1; $f = function () use (&$x) { $x = 7; }; $f(); echo $x;",
            b"7".as_slice(),
            "escaped_reference",
        ),
    ];

    for (name, source, output, expected_state) in cases {
        let result = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                quickening: QuickeningMode::On,
                inline_caches: InlineCacheMode::On,
                ..VmOptions::default()
            },
        );

        assert!(result.status.is_success(), "{name}: {:?}", result.status);
        assert_eq!(result.output.as_bytes(), output, "{name}");
        let counters = result.counters.expect("counters");
        assert!(
            counters
                .frame_alias_state
                .get(expected_state)
                .is_some_and(|count| *count >= 1),
            "{name}: {counters:?}"
        );
        assert!(
            counters.fast_path_disabled_by_reference >= 1,
            "{name}: {counters:?}"
        );
    }
}

#[test]
fn references_execute_chains_rebinding_and_unset_name_semantics() {
    let chain = execute_source("<?php $a = 1; $b =& $a; $c =& $b; $c = 3; echo $a, $b, $c;");

    assert!(chain.status.is_success(), "{:?}", chain.status);
    assert_eq!(chain.output.as_bytes(), b"333");

    let rebind =
        execute_source("<?php $a = 1; $b = 2; $c =& $a; $c =& $b; $c = 4; echo $a, $b, $c;");

    assert!(rebind.status.is_success(), "{:?}", rebind.status);
    assert_eq!(rebind.output.as_bytes(), b"144");

    let unset_name = execute_source(
        "<?php $a = 1; $b =& $a; unset($a); $b = 2; echo isset($a) ? 'bad' : 'unset', '|', $b;",
    );

    assert!(unset_name.status.is_success(), "{:?}", unset_name.status);
    assert_eq!(unset_name.output.as_bytes(), b"unset|2");
}

#[test]
fn references_lower_property_bindings_to_reference_ir() {
    let object_ref = php_ir::lower_frontend_result(
        &php_semantics::analyze_source(
            "<?php class Box { public $p = 1; } $box = new Box(); $alias =& $box->p;",
        ),
        php_ir::LoweringOptions::default(),
    );
    assert!(
        object_ref.diagnostics.is_empty(),
        "{:?}",
        object_ref.diagnostics
    );
    assert!(
        object_ref.verification.is_ok(),
        "{:?}",
        object_ref.verification
    );

    let has_property_reference = object_ref.unit.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(
                    instruction.kind,
                    InstructionKind::BindReferenceProperty { .. }
                        | InstructionKind::BindReferencePropertyDim { .. }
                        | InstructionKind::BindReferenceDimFromProperty { .. }
                        | InstructionKind::BindReferenceFromProperty { .. }
                        | InstructionKind::BindReferenceFromPropertyDim { .. }
                )
            })
        })
    });
    assert!(
        has_property_reference,
        "expected property reference binding in lowered IR"
    );
}

#[test]
fn lvalue_array_element_references_bind_selected_cell() {
    let read_through =
        execute_source("<?php $a = []; $b = 1; $a[\"x\"] =& $b; $b = 3; echo $a[\"x\"]; ");

    assert!(
        read_through.status.is_success(),
        "{:?}",
        read_through.status
    );
    assert_eq!(read_through.output.as_bytes(), b"3");

    let write_through = execute_source(
        "<?php $a = []; $b = 1; $a[\"x\"] =& $b; $a[\"x\"] = 4; echo $b, \"|\", $a[\"x\"];",
    );

    assert!(
        write_through.status.is_success(),
        "{:?}",
        write_through.status
    );
    assert_eq!(write_through.output.as_bytes(), b"4|4");
}

#[test]
fn lvalue_property_references_bind_selected_cells() {
    let property_target = execute_source(
        "<?php class C { public $p; } $c = new C(); $v = 1; $c->p =& $v; $v = 3; echo $c->p; $c->p = 5; echo '|', $v;",
    );

    assert!(
        property_target.status.is_success(),
        "{:?}",
        property_target.status
    );
    assert_eq!(property_target.output.as_bytes(), b"3|5");

    let property_source = execute_source(
        "<?php class C { public $p = 1; } $c = new C(); $a = []; $a['p'] =& $c->p; $c->p = 4; echo $a['p']; $a['p'] = 7; echo '|', $c->p;",
    );

    assert!(
        property_source.status.is_success(),
        "{:?}",
        property_source.status
    );
    assert_eq!(property_source.output.as_bytes(), b"4|7");
}

#[test]
fn property_dimension_reference_assignment_binds_property_dimension_source() {
    let result = execute_source(
        "<?php
            class C {
                public $data = ['links' => ['base' => ['a']]];
                public function run() {
                    $this->data['links']['alias'] =& $this->data['links']['base'];
                    $this->data['links']['base'][] = 'b';
                    $this->data['links']['alias'][] = 'c';
                    echo implode(',', $this->data['links']['base']);
                    echo '|';
                    echo implode(',', $this->data['links']['alias']);
                }
            }
            (new C())->run();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a,b,c|a,b,c");
}

#[test]
fn array_literal_property_reference_elements_bind_property_cell() {
    let result = execute_source(
        "<?php
            class Hooks {
                public function dispatch($args) {
                    $args[0] = 'changed';
                }
            }
            class Transport {
                public $handle = 'initial';
                public function send($hooks) {
                    $hooks->dispatch([&$this->handle]);
                    return $this->handle;
                }
            }
            echo (new Transport())->send(new Hooks());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"changed");
}

#[test]
fn by_ref_property_return_binds_property_cell() {
    let result = execute_source(
        "<?php
            class Transport {
                public $handle = 'initial';
                public function &get_handle() {
                    return $this->handle;
                }
            }
            $transport = new Transport();
            $alias =& $transport->get_handle();
            $alias = 'changed';
            echo $transport->handle;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"changed");
}

#[test]
fn dynamic_property_variable_member_isset_and_unset_execute() {
    let result = execute_source(
        "<?php
            class C {
                public $data;
                function __construct() { $this->data = (object) ['x' => 1]; }
                function has($key) {
                    var_dump(isset($this->data->$key));
                    unset($this->data->$key);
                    var_dump(isset($this->data->$key));
                }
            }
            (new C())->has('x');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "bool(true)\nbool(false)\n");
}

#[test]
fn unbraced_dynamic_property_empty_executes() {
    let result = execute_source(
        "<?php class Queried { public $post_type = 1; } function get_queried_object() { return new Queried(); } $kind = 'post_type'; var_dump(! empty( get_queried_object()->$kind ));",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "bool(true)\n");
}

#[test]
fn lvalue_array_append_by_reference_binds_new_element() {
    let result = execute_source("<?php $a = []; $b = 2; $a[] =& $b; $b = 5; echo $a[0];");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5");
}

#[test]
fn static_property_dim_reference_binds_local_alias() {
    let result = execute_source(
        "<?php class C { public static $items = ['x' => ['metadata' => null]]; public static function run() { $key = 'x'; $collection =& self::$items[$key]; $collection['metadata'] = 7; echo self::$items['x']['metadata']; } } C::run();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
}

#[test]
fn static_property_by_ref_call_argument_updates_storage() {
    let result = execute_source(
        "<?php class C { public static $value = 1; } function bump(&$value) { $value++; } bump(C::$value); echo C::$value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2");
}

#[test]
fn static_property_by_value_call_argument_does_not_update_storage() {
    let result = execute_source(
        "<?php class C { public static $value = 1; } function bump($value) { $value++; } bump(C::$value); echo C::$value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1");
}

#[test]
fn static_property_dim_increment_updates_element() {
    let result = execute_source(
        "<?php class C { public static $seen = ['menu' => 1]; public static function run($name) { ++static::$seen[$name]; echo static::$seen[$name]; } } C::run('menu');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2");
}

#[test]
fn array_literal_reference_elements_bind_local_cells() {
    let result = execute_source(
        "<?php $value = 10; $array = [1 => &$value]; $value = 20; var_dump($array);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array(1) {\n  [1]=>\n  &int(20)\n}\n"
    );
}

#[test]
fn lvalue_nested_dim_increment_and_auto_creation_execute() {
    let nested = execute_source(
        "<?php $a = [\"x\" => [\"y\" => 1]]; $a[\"x\"][\"y\"]++; echo $a[\"x\"][\"y\"];",
    );

    assert!(nested.status.is_success(), "{:?}", nested.status);
    assert_eq!(nested.output.as_bytes(), b"2");

    let auto_create = execute_source("<?php $a = []; $a[\"x\"][\"y\"] = 6; echo $a[\"x\"][\"y\"];");

    assert!(auto_create.status.is_success(), "{:?}", auto_create.status);
    assert_eq!(auto_create.output.as_bytes(), b"6");
}

#[test]
fn lvalue_unset_dimension_preserves_other_elements_and_alias_cells() {
    let result = execute_source(
        "<?php $a = [\"x\" => 1, \"y\" => 2]; $r =& $a[\"x\"]; unset($a[\"x\"]); $r = 7; echo isset($a[\"x\"]) ? \"bad\" : \"unset\", \"|\", $a[\"y\"], \"|\", $r;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"unset|2|7");

    let dynamic_key = execute_source(
        "<?php $closure = function() { return \"k\"; }; $a = [\"k\" => [\"d\" => 1]]; unset($a[$closure()][\"d\"]); echo isset($a[\"k\"][\"d\"]) ? \"bad\" : \"unset\";",
    );

    assert!(dynamic_key.status.is_success(), "{:?}", dynamic_key.status);
    assert_eq!(dynamic_key.output.as_bytes(), b"unset");
    assert!(
        dynamic_key.diagnostics.is_empty(),
        "{:?}",
        dynamic_key.diagnostics
    );
}

#[test]
fn lvalue_array_element_reference_separates_cow_copy() {
    let result = execute_source(
        "<?php $a = [\"x\" => 1]; $b = $a; $r = 9; $b[\"x\"] =& $r; echo $a[\"x\"], \"|\", $b[\"x\"];",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|9");
}

#[test]
fn lvalue_trace_records_array_dimension_paths() {
    let result = execute_source_with_options(
        "<?php $a = []; $b = 1; $a[\"x\"] =& $b;",
        VmOptions {
            trace: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert!(
        result.trace.iter().any(
            |event| event.contains("lvalue operation=bind-reference-dim")
                && event.contains("path=[string(\"x\")]")
        ),
        "{:?}",
        result.trace
    );
}

#[test]
fn trace_runtime_records_reference_cow_snapshot() {
    let result = execute_source_with_options(
        "<?php $a = [\"x\" => 1]; $b = $a; $r = 9; $b[\"x\"] =& $r; echo $a[\"x\"], \"|\", $b[\"x\"];",
        VmOptions {
            trace_runtime: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|9");
    let events = runtime_trace_events(&result.trace);
    assert_eq!(
        events,
        vec![
            "lvalue operation=bind-reference-dim local=1 path=[string(\"x\")]".to_owned(),
            "gc-roots roots=0 entities=0 cycle_candidates=0".to_owned(),
        ]
    );
    assert_trace_is_normalized(&result.trace);
}

#[test]
fn trace_runtime_records_foreach_snapshot() {
    let result = execute_source_with_options(
        "<?php foreach ([\"a\" => 1, \"b\" => 2] as $key => $value) { echo $key, $value; }",
        VmOptions {
            trace_runtime: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a1b2");
    let events = runtime_trace_events(&result.trace);
    assert_eq!(
        events,
        vec![
            "foreach init iterator=r5 kind=array-handle".to_owned(),
            "foreach next iterator=r5 status=value key=String(\"a\") value=Int(1)".to_owned(),
            "foreach next iterator=r5 status=value key=String(\"b\") value=Int(2)".to_owned(),
            "foreach next iterator=r5 status=done".to_owned(),
            "gc-roots roots=0 entities=0 cycle_candidates=0".to_owned(),
        ]
    );
    assert_trace_is_normalized(&result.trace);
}

#[test]
fn trace_runtime_records_generator_suspend_snapshot() {
    let result = execute_source_with_options(
        "<?php function gen() { yield \"k\" => \"v\"; } $g = gen(); echo $g->current();",
        VmOptions {
            trace_runtime: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"v");
    let events = runtime_trace_events(&result.trace);
    assert_eq!(
        events,
        vec![
            "generator state function=1 transition=created->running".to_owned(),
            "generator suspend function=1 key=String(\"k\") value=String(\"v\")".to_owned(),
            "gc-roots roots=0 entities=0 cycle_candidates=0".to_owned(),
        ]
    );
    assert_trace_is_normalized(&result.trace);
}

#[test]
fn trace_runtime_records_fiber_suspend_snapshot() {
    let result = execute_source_with_options(
        "<?php $fiber = new Fiber(function() { echo \"a\"; Fiber::suspend(\"s\"); echo \"b\"; }); echo $fiber->start(); echo \"|\"; $fiber->resume(\"r\");",
        VmOptions {
            trace_runtime: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"as|b");
    let events = runtime_trace_events(&result.trace);
    assert_eq!(
        events,
        vec![
            "fiber start transition=not-started->running".to_owned(),
            "fiber suspend transition=running->suspended state=Running value=String(\"s\")"
                .to_owned(),
            "fiber start transition=running->suspended value=String(\"s\")".to_owned(),
            "fiber resume transition=suspended->running input=String(\"r\")".to_owned(),
            "fiber resume transition=running->terminated".to_owned(),
            "gc-roots roots=0 entities=0 cycle_candidates=0".to_owned(),
        ]
    );
    assert_trace_is_normalized(&result.trace);
}

#[test]
fn control_flow_executes_if_else_and_nested_if() {
    let result = execute_source(
        "<?php $a = 0; if (false) { echo \"bad\"; } else { if (true) { echo \"ok\"; } }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn exit_inside_function_terminates_script() {
    let message_exit = execute_source(
        "<?php function stop_now() { echo \"before|\"; exit(\"halt\"); echo \"bad\"; } echo \"start|\"; stop_now(); echo \"bad\";",
    );

    assert!(
        message_exit.status.is_success(),
        "{:?}",
        message_exit.status
    );
    assert_eq!(message_exit.output.as_bytes(), b"start|before|halt");
    assert_eq!(message_exit.process_exit_code, Some(0));
    assert_eq!(message_exit.return_value, None);

    let code_exit = execute_source(
        "<?php function stop_with_code() { echo \"before|\"; exit(3); echo \"bad\"; } echo \"start|\"; stop_with_code(); echo \"bad\";",
    );

    assert!(code_exit.status.is_success(), "{:?}", code_exit.status);
    assert_eq!(code_exit.output.as_bytes(), b"start|before|");
    assert_eq!(code_exit.process_exit_code, Some(3));
    assert_eq!(code_exit.return_value, None);
}

#[test]
fn control_flow_executes_while_do_and_for_loops() {
    let result = execute_source(
        "<?php $i = 0; while ($i < 3) { echo $i; $i++; } do { echo \"d\"; $i--; } while ($i > 2); for ($j = 0; $j < 3; $j++) { echo $j; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"012d012");
}

#[test]
fn control_flow_executes_break_and_continue() {
    let result = execute_source(
        "<?php $i = 0; while ($i < 5) { $i++; if ($i == 2) { continue; } if ($i == 4) { break; } echo $i; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"13");
}

#[test]
fn short_circuit_skips_rhs_side_effects() {
    let result = execute_source(
        "<?php $x = 0; $y = 0; echo ($x && ++$y) ? \"bad\" : \"ok\"; echo $y; echo \"|\"; echo (true || ++$y) ? \"ok\" : \"bad\"; echo $y;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok0|ok0");
}

#[test]
fn short_circuit_coalesce_condition_uses_explicit_false_target() {
    let result = execute_source(
        "<?php $dsn = \"mysql://u:p@127.0.0.1:13306/db\"; if ($dsn === false || $dsn === \"\") { echo \"empty\"; exit; } $parts = parse_url($dsn); if ($parts === false || ($parts[\"scheme\"] ?? \"\") !== \"mysql\") { echo \"invalid\"; exit; } echo \"ok\";",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn coalesce_nested_dim_fetch_treats_scalar_intermediate_as_null() {
    let result = execute_source(
        "<?php $metadata = ['supports' => ['color' => 1]]; var_export($metadata['supports']['color']['__experimentalDuotone'] ?? null); echo '|'; $metadata = ['supports' => 1]; var_export($metadata['supports']['color']['__experimentalDuotone'] ?? null); echo '|'; $metadata = []; var_export($metadata['supports']['color']['__experimentalDuotone'] ?? null);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"NULL|NULL|NULL");
}

#[test]
fn coalesce_property_nested_dim_fetch_treats_scalar_intermediate_as_null() {
    let result = execute_source(
        "<?php class Registry { protected $all = []; public function run() { $this->all['present'] = ['en' => false]; var_export($this->all['present']['en'] ?? 'fallback'); echo '|'; $this->all['scalar'] = false; echo $this->all['scalar']['en'] ?? 'fallback'; echo '|'; echo $this->all['missing']['en'] ?? 'fallback'; } } (new Registry())->run();",
    );

    assert!(
        result.status.is_success(),
        "{:?}\n{}",
        result.status,
        result.output.to_string_lossy()
    );
    assert_eq!(result.output.as_bytes(), b"false|fallback|fallback");
}

#[test]
fn coalesce_property_nested_dim_fetch_treats_missing_dynamic_key_as_null() {
    let result = execute_source(
        "<?php class Registry { protected $all = []; public function run($domain, $locale) { echo $this->all[$domain][$locale] ?? 'fallback'; } } (new Registry())->run('default', 'en_US');",
    );

    assert!(
        result.status.is_success(),
        "{:?}\n{}",
        result.status,
        result.output.to_string_lossy()
    );
    assert_eq!(result.output.as_bytes(), b"fallback");
}

#[test]
fn coalesce_static_property_dim_fetch_treats_missing_key_as_null() {
    let result = execute_source(
        "<?php class S { private static array $items = []; public string $id = 'dashboard'; function f() { return self::$items[$this->id] ?? ''; } } var_dump((new S())->f());",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"string(0) \"\"\n");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn control_flow_executes_switch_match_ternary_coalesce_and_return() {
    let result = execute_source(
        "<?php $x = 0; switch ($x) { case 0: echo \"zero\"; case 1: echo \"one\"; break; default: echo \"default\"; } echo \"|\"; echo match ($x) { 0 => \"match\", default => \"default\" }; echo \"|\"; echo $missing ?? \"fallback\"; echo \"|\"; echo true ? \"yes\" : \"no\"; return \"done\"; echo \"bad\";",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"zeroone|match|fallback|yes");
    assert_eq!(
        result.return_value,
        Some(Value::String(php_runtime::api::PhpString::from_test_str(
            "done"
        )))
    );
}

#[test]
fn switch_without_default_exits_after_unmatched_grouped_cases() {
    let result = execute_source(
        r#"<?php
            class SwitchFallbackDatabaseProbe {
                public $options = 'app_options';
                public function strip_invalid_text_for_column($table, $column, $value) {
                    echo 'wrong';
                    return $value;
                }
            }
            $database = new SwitchFallbackDatabaseProbe();
            function switch_fallback_is_error($value) { return false; }
            function switch_fallback_sanitize_email($email) { echo 'email'; return $email; }
            function switch_fallback_sanitize_option($option, $value) {
                global $database;
                $original_value = $value;
                $error = null;
                switch ($option) {
                    case 'admin_email':
                    case 'new_admin_email':
                        $value = $database->strip_invalid_text_for_column($database->options, 'option_value', $value);
                        if (switch_fallback_is_error($value)) {
                            $error = 'err';
                        } else {
                            $value = switch_fallback_sanitize_email($value);
                        }
                        break;
                    case 'thumbnail_size_w':
                    case 'thumbnail_size_h':
                    case 'medium_size_w':
                    case 'medium_size_h':
                    case 'medium_large_size_w':
                    case 'medium_large_size_h':
                    case 'large_size_w':
                    case 'large_size_h':
                    case 'mailserver_port':
                    case 'comment_max_links':
                    case 'page_on_front':
                    case 'page_for_posts':
                    case 'rss_excerpt_length':
                    case 'default_category':
                        $value = (int) $value;
                        break;
                }
                return $value;
            }
            $result = switch_fallback_sanitize_option('cron', ['version' => 2]);
            echo gettype($result), '|', $result['version'];
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"array|2");
}

#[test]
fn control_flow_match_no_arm_is_stable_runtime_error() {
    let result = execute_source("<?php echo match (2) { 0 => \"zero\", 1 => \"one\" };");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let message = result.status.message().expect("runtime error message");
    assert!(message.contains("E_PHP_VM_UNCAUGHT_EXCEPTION"));
    assert!(message.contains("UnhandledMatchError"));
    assert!(message.contains("match expression did not match any arm"));
}

#[test]
fn functions_execute_user_calls_locals_recursion_and_null_return() {
    let result = execute_source(
        "<?php function add($a, $b) { $local = $a + $b; return $local; } function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } function empty_return() { return; } $x = 10; echo add(2, 3), \"|\", fact(5), \"|\"; echo empty_return(), \"|\", $x;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5|120||10");
}

#[test]
fn functions_runtime_errors_include_call_stack() {
    let result =
        execute_source("<?php function boom() { echo 1 / 0; } function wrap() { boom(); } wrap();");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let message = result.status.message().expect("runtime error message");
    assert!(message.contains("division by zero"), "{message}");
    assert!(message.contains("call_stack:"), "{message}");
    assert!(message.contains("at boom"), "{message}");
    assert!(message.contains("at wrap"), "{message}");
    assert!(message.contains("at main"), "{message}");
}

#[test]
fn function_params_defaults_and_variadics_execute() {
    let result = execute_source(
        "<?php function greet($name = \"world\", $punct = \"!\") { echo \"hi \", $name, $punct; } function sum(...$xs) { return $xs[0] + $xs[1]; } greet(); echo \"|\"; greet(\"php\", \"?\"); echo \"|\", sum(2, 3);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"hi world!|hi php?|5");
}

#[test]
fn call_binding_named_defaults_unpacks_variadics_and_callables_execute() {
    let result = execute_source(
        "<?php function joiner($first, $second = \"B\", ...$rest) { echo $first, \"|\", $second, \"|\", $rest[0], \"|\", $rest[\"third\"]; } joiner(\"A\", ...[\"C\", \"D\"], third: \"E\"); echo \";\"; function defaults($a = \"A\", $b = \"B\", $c = \"C\") { echo $a, $b, $c; } defaults(c: \"Z\"); echo \";\"; $len = strlen(...); echo $len(...[\"hello\"]);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"A|C|D|E;ABZ;5");
}

#[test]
fn call_binding_named_by_ref_arguments_mutate_caller_local() {
    let result = execute_source(
        "<?php function set_named(&$value, $next = 5) { $value = $next; } $a = 1; set_named(value: $a, next: 7); echo $a;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
}

#[test]
fn call_binding_unpacked_array_references_mutate_by_ref_params() {
    let result = execute_source(
        "<?php
            class Hooks {
                public function before_request($url, &$headers, &$data, &$type, &$options) {
                    $headers['Cookie'] = 'a=b';
                    $data = 'body';
                    $type = 'POST';
                    $options['seen'] = true;
                }
            }
            $url = 'https://example.test/';
            $headers = [];
            $data = null;
            $type = 'GET';
            $options = [];
            $parameters = [&$url, &$headers, &$data, &$type, &$options];
            $parameters = array_values($parameters);
            $callback = [new Hooks(), 'before_request'];
            $callback(...$parameters);
            echo $headers['Cookie'], '|', $data, '|', $type, '|', ($options['seen'] ? 'yes' : 'no');
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a=b|body|POST|yes");
}

#[test]
fn call_binding_unpacked_array_references_do_not_alias_value_params() {
    let result = execute_source(
        "<?php
            function overwrite_value($value) {
                $value = 'inner';
            }
            $value = 'outer';
            $parameters = [&$value];
            overwrite_value(...array_values($parameters));
            echo $value, '|', $parameters[0];
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"outer|outer");
}

#[test]
fn dynamic_class_static_method_call_assigns_return_value() {
    let result = execute_source(
        "<?php
            class DynamicTransportProbe {
                public static function test($capabilities = []) {
                    return empty($capabilities) ? true : false;
                }
            }
            $class = 'DynamicTransportProbe';
            $value = $class::test([]);
            var_dump(isset($value));
            var_dump($value);
            $other = $class::test(['ssl' => false]);
            var_dump(isset($other));
            var_dump($other);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"bool(true)\nbool(true)\nbool(true)\nbool(false)\n"
    );
}

#[test]
fn call_binding_optional_by_ref_default_is_local_value() {
    let result = execute_source(
        "<?php function cache_get($key, &$found = null) { echo $found === null ? 'null' : 'set'; $found = true; echo '|', $found ? 'true' : 'false'; } cache_get('k');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"null|true");
}

#[test]
fn call_binding_named_argument_errors_are_stable() {
    let unknown = execute_source("<?php function one($value) { return $value; } one(missing: 1);");
    assert_eq!(unknown.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(unknown.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        unknown
            .output
            .to_string_lossy()
            .contains("Uncaught Error: Unknown named parameter $missing"),
        "{}",
        unknown.output.to_string_lossy()
    );

    let duplicate =
        execute_source("<?php function one($value) { return $value; } one(1, value: 2);");
    assert_eq!(duplicate.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(duplicate.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        duplicate
            .output
            .to_string_lossy()
            .contains("Uncaught Error: Named parameter $value overwrites previous argument"),
        "{}",
        duplicate.output.to_string_lossy()
    );

    let positional_after_named =
        execute_source("<?php function pair($a, $b) { return $a; } pair(a: 1, 2);");
    assert_eq!(
        positional_after_named.status.exit_status(),
        ExitStatus::RuntimeError
    );
    assert_eq!(
        positional_after_named.diagnostics[0].id(),
        "E_PHP_VM_POSITIONAL_AFTER_NAMED_ARG"
    );

    let unpack_non_array =
        execute_source("<?php function one($value) { return $value; } one(...4);");
    assert_eq!(
        unpack_non_array.status.exit_status(),
        ExitStatus::RuntimeError
    );
    assert_eq!(
        unpack_non_array.diagnostics[0].id(),
        "E_PHP_VM_UNPACK_NON_ARRAY"
    );
}

#[test]
fn function_params_argument_count_errors_are_stable() {
    let missing = execute_source("<?php function one($a) { return $a; } one();");
    assert_eq!(missing.status.exit_status(), ExitStatus::RuntimeError);
    let missing_message = missing.status.message().unwrap_or_default();
    assert!(
            missing_message.contains(
                "Uncaught ArgumentCountError: Too few arguments to function one(), 0 passed in /tmp/phrust-test.php on line "
            ),
            "{missing_message}"
        );
    // getMessage() ends after the expectation (reference behavior); the
    // uncaught fatal rendering appends the declaration site.
    assert!(
        missing_message.ends_with(" and exactly 1 expected"),
        "{missing_message}"
    );
    let missing_output = missing.output.to_string_lossy();
    assert!(
        missing_output.contains(" and exactly 1 expected in /tmp/phrust-test.php:"),
        "{missing_output}"
    );

    let strict_type_before_missing = execute_source(
        "<?php declare(strict_types=1); function pair(int $a, string $b) {} pair('1');",
    );
    assert_eq!(
        strict_type_before_missing.status.exit_status(),
        ExitStatus::RuntimeError
    );
    let strict_message = strict_type_before_missing
        .status
        .message()
        .unwrap_or_default();
    assert!(
        strict_message.contains(
            "Uncaught TypeError: pair(): Argument #1 ($a) must be of type int, string given"
        ),
        "{strict_message}"
    );

    // PHP accepts extra positional arguments to a non-variadic function;
    // they are ignored for binding but visible to func_get_args().
    let extra = execute_source(
        "<?php function one($a) { return $a . '|' . implode(',', func_get_args()); } echo one(1, 2);",
    );
    assert!(extra.status.is_success(), "{:?}", extra.status);
    assert_eq!(extra.output.as_bytes(), b"1|1,2");
}

#[test]
fn weak_by_ref_scalar_params_coerce_and_write_back() {
    let result = execute_source(
        "<?php function takes_int(int &$value): void { echo gettype($value), ':', $value, '|'; } $value = '42'; takes_int($value); echo gettype($value), ':', $value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"integer:42|integer:42");

    let float_result = execute_source(
        "<?php function takes_float(float &$value): void { echo gettype($value), ':', $value, '|'; } $value = 1; takes_float($value); echo gettype($value), ':', $value;",
    );

    assert!(
        float_result.status.is_success(),
        "{:?}",
        float_result.status
    );
    assert_eq!(float_result.output.as_bytes(), b"double:1|double:1");
}

#[test]
fn strict_int_to_float_params_and_returns_materialize_float() {
    let result = execute_source(
        "<?php declare(strict_types=1); function takes_float(float $value): float { echo gettype($value), ':', $value, '|'; return 1; } $result = takes_float(1); echo gettype($result), ':', $result;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"double:1|double:1");
}

#[test]
fn frame_call_site_lines_feed_debug_backtrace() {
    let function_call = execute_temp_source_file(
        "frame-function-call",
        "<?php\nfunction leaf() {\n    $trace = debug_backtrace();\n    echo $trace[0]['line'];\n}\nleaf();\n",
    );
    assert!(
        function_call.status.is_success(),
        "{:?}",
        function_call.status
    );
    assert_eq!(function_call.output.as_bytes(), b"6");

    let method_call = execute_temp_source_file(
        "frame-method-call",
        "<?php\nclass Box {\n    public function leaf() {\n        $trace = debug_backtrace();\n        echo $trace[0]['line'];\n    }\n}\n$box = new Box();\n$box->leaf();\n",
    );
    assert!(method_call.status.is_success(), "{:?}", method_call.status);
    assert_eq!(method_call.output.as_bytes(), b"9");

    let static_call = execute_temp_source_file(
        "frame-static-call",
        "<?php\nclass Box {\n    public static function leaf() {\n        $trace = debug_backtrace();\n        echo $trace[0]['line'];\n    }\n}\nBox::leaf();\n",
    );
    assert!(static_call.status.is_success(), "{:?}", static_call.status);
    assert_eq!(static_call.output.as_bytes(), b"8");
}

#[test]
fn function_params_return_type_success_and_failure() {
    let success = execute_source(
        "<?php function text(): string { return \"ok\"; } function number(): int { return 4; } function nothing(): void { return; } echo text(), \"|\", number(), \"|\"; echo nothing(), \"x\";",
    );
    assert!(success.status.is_success(), "{:?}", success.status);
    assert_eq!(success.output.as_bytes(), b"ok|4|x");

    let failure = execute_source("<?php function bad(): int { return \"no\"; } bad();");
    assert_eq!(failure.status.exit_status(), ExitStatus::RuntimeError);
    let message = failure.status.message().expect("runtime error message");
    assert!(
        message.contains("E_PHP_VM_RETURN_TYPE_MISMATCH"),
        "{message}"
    );
    assert!(
        message.contains("function bad returned string, expected int"),
        "{message}"
    );
}

#[test]
fn return_type_weakly_coerces_scalar_values() {
    let result = execute_source(
        "<?php function safe_to_string(int|float $number): string { return $number; } echo safe_to_string(5.5);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"5.5");
}

#[test]
fn runtime_types_check_scalar_params_and_nullable_values() {
    let success = execute_source(
        "<?php function add_one(int $value): int { return $value + 1; } function label(?string $value): string { if ($value === null) { return 'none'; } return $value; } echo add_one(4), '|', label(null), '|', label('ok');",
    );
    assert!(success.status.is_success(), "{:?}", success.status);
    assert_eq!(success.output.as_bytes(), b"5|none|ok");

    let failure = execute_source(
        "<?php function add_one(int $value): int { return $value + 1; } add_one([]);",
    );
    // An uncaught argument type error surfaces as PHP's uncaught `TypeError`.
    assert_eq!(failure.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(failure.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
            failure.output.to_string_lossy().contains(
                "Uncaught TypeError: add_one(): Argument #1 ($value) must be of type int, array given, called in "
            ),
            "{}",
            failure.output.to_string_lossy()
        );
    let failure_output = failure.output.to_string_lossy();
    assert!(
        failure_output.contains(" and defined in "),
        "{failure_output}"
    );
    assert!(
        failure_output.contains("add_one(Array)"),
        "{failure_output}"
    );
    assert!(failure_output.contains("  thrown in "), "{failure_output}");

    let union_failure = execute_source(
        "<?php function label(int|string $value): string { return 'ok'; } label([]);",
    );
    assert_eq!(union_failure.status.exit_status(), ExitStatus::RuntimeError);
    let union_output = union_failure.output.to_string_lossy();
    assert!(
            union_output.contains(
                "Uncaught TypeError: label(): Argument #1 ($value) must be of type string|int, array given"
            ),
            "{union_output}"
        );
}

#[test]
fn callback_type_error_can_be_caught_as_error_parent() {
    let result = execute_source(
        "<?php class A {} function accepts_a(A $a) {} try { call_user_func('accepts_a', 1); } catch (Error $e) { echo get_class($e), ':', $e->getCode(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"TypeError:0");
}

#[test]
fn caught_error_method_calls_work_inside_string_interpolation() {
    let result = execute_source(
        "<?php class A {} function accepts_a(A $a) {} try { call_user_func('accepts_a', 1); } catch (Error $ex) { echo \"{$ex->getCode()}: {$ex->getMessage()}\"; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with("0: accepts_a(): Argument #1 ($a)"),
        "{output}"
    );
    assert!(output.contains("must be of type A, int given"), "{output}");
}

#[test]
fn braced_property_interpolation_reads_current_object_property() {
    let result = execute_source(
        "<?php class A { private $dir; function __construct($dir) { $this->dir = $dir; } function __invoke($class) { echo \"A('{$this->dir}') $class\\n\"; } } $a = new A('d1'); $a('TestX');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"A('d1') TestX\n");
}

#[test]
fn braced_property_chain_interpolation_reads_nested_property() {
    let result = execute_source(
        "<?php class Screen { public $id = 'dashboard'; } class Table { public $screen; function __construct() { $this->screen = new Screen(); } function hook() { echo \"manage_{$this->screen->id}_columns\"; } } (new Table())->hook();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"manage_dashboard_columns");
}

#[test]
fn interpolated_dynamic_method_name_calls_resolved_method() {
    let result = execute_source(
        "<?php class IriProbe { function __get($name) { return $this->{\"get_$name\"}(); } function get_iri() { return 'iri'; } } echo (new IriProbe())->iri;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"iri");
}

#[test]
fn braced_array_dim_chain_interpolation_reads_nested_dimension() {
    let result = execute_source(
        "<?php $submenu_items = [[0, 1, 'index.php']]; echo \"{$submenu_items[0][2]}\";",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"index.php");
}

#[test]
fn braced_property_dim_interpolation_reads_dimension() {
    let result = execute_source(
        "<?php class A { public $rewrite = ['slug' => 'category']; function render() { echo \"{$this->rewrite['slug']}\"; } } (new A())->render();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"category");
}

#[test]
fn runtime_types_check_returns_void_and_properties() {
    let success = execute_source(
        "<?php class Box { public int $value; } function text(): string { return 'ok'; } function done(): void { return; } $box = new Box(); $box->value = 7; echo text(), '|', done(), '|', $box->value;",
    );
    assert!(success.status.is_success(), "{:?}", success.status);
    assert_eq!(success.output.as_bytes(), b"ok||7");

    let bad_return = execute_source("<?php function text(): string { return []; } text();");
    assert_eq!(bad_return.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        bad_return.diagnostics[0].id(),
        "E_PHP_VM_RETURN_TYPE_MISMATCH"
    );

    let bad_property = execute_source(
        "<?php class Box { public int $value; } $box = new Box(); $box->value = [];",
    );
    assert_eq!(bad_property.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        bad_property.diagnostics[0].id(),
        "E_PHP_VM_UNCAUGHT_EXCEPTION"
    );
    assert!(
        bad_property.output.to_string_lossy().contains(
            "Uncaught TypeError: Cannot assign array to property Box::$value of type int"
        ),
        "{}",
        bad_property.output.to_string_lossy()
    );

    let caught_property = execute_source(
        "<?php class Box { public int $value; } $box = new Box(); try { $box->value = []; } catch (TypeError $e) { echo 'type'; }",
    );
    assert!(
        caught_property.status.is_success(),
        "{:?}",
        caught_property.status
    );
    assert_eq!(caught_property.output.as_bytes(), b"type");

    let bad_void =
        php_semantics::analyze_source("<?php function bad(): void { return null; } bad();");
    assert!(bad_void.has_errors());
    assert!(
        bad_void
            .semantic_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.id().as_str() == "E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION")
    );
}

#[test]
fn typecheck_fast_paths_match_slow_path_for_common_prologues() {
    let source = "<?php declare(strict_types=1);
            class Base {}
            class Child extends Base {}
            function scalars(int $i, string $s, bool $b, float $f, array $a, object $o, callable $cb): string {
                if (!$b) { return 'bad'; }
                return $s . $i . ':' . $f . ':' . $a[0] . ':' . $cb();
            }
            function mutate(int &$n, ?string $label = null, bool $flag = true): int {
                if ($label !== null || !$flag) { return 0; }
                $n = $n + 1;
                return $n;
            }
            function collect(string $first = 'x', string ...$rest): string {
                return $first . $rest[0] . $rest['tail'];
            }
            function exact(Base $b): string { return 'base'; }
            $n = 1;
            echo scalars(7, 's', true, 1.5, [9], new Base(), fn() => 'call');
            echo '|', mutate(n: $n), ':', $n;
            echo '|', collect('A', ...['B'], tail: 'C');
            echo '|', exact(new Base()), ':', exact(new Child());";

    let fast = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: true,
            ..VmOptions::default()
        },
    );
    let slow = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: false,
            ..VmOptions::default()
        },
    );

    assert!(fast.status.is_success(), "{:?}", fast.status);
    assert_eq!(fast.output.as_bytes(), slow.output.as_bytes());
    assert_eq!(fast.output.as_bytes(), b"s7:1.5:9:call|2:2|ABC|base:base");
    let fast_counters = fast.counters.expect("fast counters");
    let slow_counters = slow.counters.expect("slow counters");
    assert!(
        fast_counters.typecheck_fast_path_hits > 0,
        "{fast_counters:?}"
    );
    assert!(
        fast_counters.typecheck_fast_path_misses > 0,
        "{fast_counters:?}"
    );
    assert_eq!(slow_counters.typecheck_fast_path_hits, 0);
    assert_eq!(slow_counters.typecheck_fast_path_misses, 0);
}

#[test]
fn typecheck_fast_paths_preserve_coercion_by_ref_and_return_errors() {
    let weak = "<?php function takes_int(int $value): int { return $value; } echo takes_int('42');";
    let weak_fast = execute_source_with_options(
        weak,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: true,
            ..VmOptions::default()
        },
    );
    let weak_slow = execute_source_with_options(
        weak,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: false,
            ..VmOptions::default()
        },
    );
    assert!(weak_fast.status.is_success(), "{:?}", weak_fast.status);
    assert_eq!(weak_fast.output.as_bytes(), weak_slow.output.as_bytes());
    assert_eq!(weak_fast.output.as_bytes(), b"42");
    assert!(
        weak_fast
            .counters
            .expect("weak counters")
            .typecheck_fast_path_hits
            > 0
    );

    // An uncaught argument type error propagates out of the call frame and
    // renders as PHP's uncaught `TypeError`.
    let strict = "<?php declare(strict_types=1); function takes_int(int $value): int { return $value; } takes_int('42');";
    assert_typecheck_fast_path_error_matches_slow_path(strict, "E_PHP_VM_UNCAUGHT_EXCEPTION");

    let by_ref = "<?php function takes_ref(int &$value): void {} $value = '42'; takes_ref($value); echo gettype($value), ':', $value;";
    let by_ref_fast = execute_source_with_options(
        by_ref,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: true,
            ..VmOptions::default()
        },
    );
    let by_ref_slow = execute_source_with_options(
        by_ref,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: false,
            ..VmOptions::default()
        },
    );
    assert!(by_ref_fast.status.is_success(), "{:?}", by_ref_fast.status);
    assert_eq!(by_ref_fast.output.as_bytes(), by_ref_slow.output.as_bytes());
    assert_eq!(by_ref_fast.output.as_bytes(), b"integer:42");

    let variadic = "<?php function ints(int ...$xs): int { return $xs[0] + $xs[1]; } echo ints('40', 2), '|'; ints('bad');";
    let variadic_result = execute_source_with_options(
        variadic,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: true,
            ..VmOptions::default()
        },
    );
    assert_eq!(
        variadic_result.status.exit_status(),
        ExitStatus::RuntimeError
    );
    assert_eq!(
        variadic_result.diagnostics[0].id(),
        "E_PHP_VM_UNCAUGHT_EXCEPTION"
    );
    assert!(
        variadic_result
            .output
            .to_string_lossy()
            .contains("Uncaught TypeError: ints(): Argument #1 must be of type int"),
        "{}",
        variadic_result.output.to_string_lossy()
    );

    let return_error = "<?php function bad(): int { return 'x'; } bad();";
    assert_typecheck_fast_path_error_matches_slow_path(
        return_error,
        "E_PHP_VM_RETURN_TYPE_MISMATCH",
    );
}

fn assert_typecheck_fast_path_error_matches_slow_path(source: &str, expected_id: &str) {
    let fast = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: true,
            ..VmOptions::default()
        },
    );
    let slow = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            typecheck_fast_paths: false,
            ..VmOptions::default()
        },
    );

    assert_eq!(fast.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(slow.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(fast.status.message(), slow.status.message());
    assert_eq!(fast.diagnostics[0].id(), expected_id);
    assert_eq!(slow.diagnostics[0].id(), expected_id);
    let counters = fast.counters.expect("fast counters");
    assert!(counters.typecheck_fast_path_misses > 0, "{counters:?}");
}

#[test]
fn internal_function_dispatch_cache_preserves_normal_output_and_records_hits() {
    let source = "<?php
            $items = [1, 2, 3];
            echo count($items), ':', strlen('abcd'), ':', (is_int(7) ? 'i' : 'n'), ':', implode(',', array_values(['a' => 1, 'b' => 2])), \"\\n\";
            echo count($items), ':', strlen('ef'), ':', (is_string('x') ? 's' : 'n'), ':', implode(',', array_values(['c' => 3, 'd' => 4])), \"\\n\";
            echo count($items), ':', strlen('ghij'), ':', (is_array($items) ? 'a' : 'n'), ':', strtolower('ABC'), \"\\n\";
            echo function_exists('strlen') ? 'exists|' : 'missing|';
            $rf = new ReflectionFunction('strlen');
            echo $rf->getName(), ':', ($rf->isInternal() ? 'internal' : 'user');
        ";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            internal_function_dispatch_cache: false,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            internal_function_dispatch_cache: true,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(
        on.output.as_bytes(),
        b"3:4:i:1,2\n3:2:s:3,4\n3:4:a:abc\nexists|strlen:internal"
    );
    let off_counters = off.counters.expect("off counters");
    let on_counters = on.counters.expect("on counters");
    assert_eq!(off_counters.internal_function_dispatch_cache_hits, 0);
    assert_eq!(off_counters.internal_function_dispatch_cache_misses, 0);
    assert!(
        on_counters.internal_function_dispatches >= 12,
        "{on_counters:?}"
    );
    assert!(
        on_counters.internal_function_dispatch_cache_hits >= 4,
        "{on_counters:?}"
    );
    assert!(
        on_counters.internal_function_dispatch_cache_misses >= 6,
        "{on_counters:?}"
    );
    assert!(
        on_counters.internal_count_array_direct_fast_path_hits >= 3,
        "{on_counters:?}"
    );
}

#[test]
fn builtin_intrinsics_preserve_string_semantics_and_record_counters() {
    let source = "<?php
            echo str_contains(\"ab\\0cd\", \"\\0c\") ? 'contains' : 'missing';
            echo '|', str_contains(\"abc\", \"\") ? 'empty' : 'bad';
            echo '|', str_starts_with(\"abcdef\", \"abc\") ? 'start' : 'bad';
            echo '|', str_ends_with(\"abcdef\", \"def\") ? 'end' : 'bad';
            echo '|', strtolower(\"A\\0Z!\");
            echo '|', str_contains(\"abc\", 2) ? 'coerced' : 'not';
            echo '|', str_contains(needle: 'z', haystack: 'abc') ? 'bad' : 'named';
            echo '|', hash('xxh32', 'Lorem ipsum dolor sit amet, consectetur adipiscing elit.', options: ['seed' => 42]);
            $hash_ctx = hash_init('xxh64', options: ['seed' => 42]);
            hash_update($hash_ctx, 'Lorem ipsum dolor sit amet, consectetur adipiscing elit.');
            echo '|', hash_final($hash_ctx);
            $shape = ['answer' => 42, 3 => 'three'];
            echo '|', array_key_exists('answer', $shape) ? 'ake-hit' : 'bad';
            echo '|', array_key_exists('missing', $shape) ? 'bad' : 'ake-miss';
            try { array_key_exists([], $shape); } catch (TypeError $e) { echo '|ake-type'; }
        ";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    assert_eq!(on.output, off.output);
    assert_eq!(
            on.output.as_bytes(),
            b"contains|empty|start|end|a\0z!|not|named|3d0cc7e5|9c9aa071b5d22a15|ake-hit|ake-miss|ake-type"
        );
    let off_counters = off.counters.expect("off counters");
    let on_counters = on.counters.expect("on counters");
    assert_eq!(off_counters.builtin_intrinsic_candidates, 0);
    assert!(on_counters.builtin_intrinsic_candidates >= 6);
    for name in [
        "str_contains",
        "str_starts_with",
        "str_ends_with",
        "strtolower",
        "array_key_exists",
    ] {
        assert!(
            on_counters.intrinsic_hits.get(name).copied().unwrap_or(0) > 0,
            "{name}: {on_counters:?}"
        );
    }
    assert_eq!(
        on_counters
            .intrinsic_fallback_by_reason
            .get("str_contains.type"),
        Some(&1)
    );
    assert_eq!(
        on_counters
            .intrinsic_fallback_by_reason
            .get("array_key_exists.type"),
        Some(&1)
    );
}

#[test]
fn is_numeric_intrinsic_matches_generic_and_fires() {
    let source = "<?php
            echo is_numeric(42) ? '1' : '0';
            echo is_numeric(1.5) ? '1' : '0';
            echo is_numeric(\"123\") ? '1' : '0';
            echo is_numeric(\"1.5e3\") ? '1' : '0';
            echo is_numeric(\"12abc\") ? '1' : '0';
            echo is_numeric(\"abc\") ? '1' : '0';
            echo is_numeric(\"\") ? '1' : '0';
            echo is_numeric(true) ? '1' : '0';
            echo is_numeric(null) ? '1' : '0';
            echo is_numeric([]) ? '1' : '0';
        ";
    let off = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            inline_caches: InlineCacheMode::Off,
            ..VmOptions::default()
        },
    );
    let on = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(off.status.is_success(), "{:?}", off.status);
    assert!(on.status.is_success(), "{:?}", on.status);
    // Behavior-preserving: the intrinsic matches the generic builtin.
    assert_eq!(on.output, off.output);
    // int, float, int-string, float-string are numeric; leading-numeric,
    // non-numeric, empty, bool, null, and array are not.
    assert_eq!(on.output.as_bytes(), b"1111000000");
    let on_counters = on.counters.expect("on counters");
    assert!(
        on_counters
            .intrinsic_hits
            .get("is_numeric")
            .copied()
            .unwrap_or(0)
            > 0,
        "is_numeric intrinsic should fire: {on_counters:?}"
    );
}

#[test]
fn internal_function_dispatch_cache_preserves_error_paths() {
    let cases = [
        (
            "<?php strlen(bytes: 'abc');",
            ExitStatus::RuntimeError,
            b"".as_slice(),
        ),
        ("<?php strlen();", ExitStatus::RuntimeError, b"".as_slice()),
        (
            "<?php try { strlen([]); } catch (TypeError $e) { echo 'type'; }",
            ExitStatus::Success,
            b"type".as_slice(),
        ),
        (
            "<?php try { str_repeat('x', -1); } catch (ValueError $e) { echo 'value'; }",
            ExitStatus::Success,
            b"value".as_slice(),
        ),
        (
            "<?php perf_741_missing_internal();",
            ExitStatus::RuntimeError,
            b"".as_slice(),
        ),
    ];

    for (source, expected_status, expected_output) in cases {
        let off = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                internal_function_dispatch_cache: false,
                ..VmOptions::default()
            },
        );
        let on = execute_source_with_options(
            source,
            VmOptions {
                collect_counters: true,
                collect_profile_spans: false,
                collect_layout_source_attribution: true,
                internal_function_dispatch_cache: true,
                ..VmOptions::default()
            },
        );

        assert_eq!(off.status.exit_status(), expected_status, "{source}");
        assert_eq!(on.status.exit_status(), expected_status, "{source}");
        assert_eq!(on.status.message(), off.status.message(), "{source}");
        assert_eq!(on.output, off.output, "{source}");
        if expected_status == ExitStatus::RuntimeError
            && expected_output.is_empty()
            && on.output.as_bytes() != expected_output
        {
            let output = on.output.to_string_lossy();
            assert!(
                output.contains("Fatal error: Uncaught "),
                "{source}: {output}"
            );
        } else {
            assert_eq!(on.output.as_bytes(), expected_output, "{source}");
        }
    }
}

#[test]
fn reflection_internal_functions_expose_stdlib_metadata() {
    let result = execute_source(
        "<?php $fn = new ReflectionFunction('count'); echo $fn->getName(), '|'; echo $fn->isInternal() ? 'internal|' : 'user|'; echo $fn->getFileName() === false ? 'nofile|' : 'file|'; echo $fn->getNumberOfParameters(), ':', $fn->getNumberOfRequiredParameters(), '|'; $params = $fn->getParameters(); echo $params[0]->getName(), ':', $params[0]->getPosition(), ':', ($params[0]->hasType() ? $params[0]->getType()->getName() : 'none'), '|'; echo $params[1]->getPosition(), '|'; echo $fn->getReturnType()->getName(), '|', $fn->getExtensionName();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"count|internal|nofile|2:1|value:0:Countable|array|1|int|standard"
    );
}

#[test]
fn reflection_internal_classes_and_methods_expose_extension_and_locations() {
    let result = execute_source(
        "<?php $class = new ReflectionClass('ArrayObject'); echo $class->getName(), '|', $class->getExtensionName(), '|'; echo $class->isInternal() ? 'internal|' : 'user|'; echo $class->getFileName() === false ? 'nofile|' : 'file|'; $method = $class->getMethod('count'); echo $method->getName(), '|', $method->getDeclaringClass()->getName(), '|'; echo $method->isInternal() ? 'internal|' : 'user|'; echo $method->getNumberOfParameters(), '|', $method->getModifiers(), '|', $method->getExtensionName();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"ArrayObject|spl|internal|nofile|count|ArrayObject|internal|0|1|spl"
    );
}

#[test]
fn reflection_user_parameters_and_method_modifiers_use_ir_metadata() {
    let result = execute_source(
        "<?php abstract class ReflectionMetaProbe { final public static function pub($a, $b = null, ...$rest): void {} abstract protected function prot(); private function priv() {} } $method = new ReflectionMethod(ReflectionMetaProbe::class, 'pub'); $params = $method->getParameters(); echo $method->getModifiers(), '|', $params[0]->getPosition(), ':', $params[1]->getPosition(), ':', $params[2]->getPosition(), '|'; echo (new ReflectionMethod(ReflectionMetaProbe::class, 'prot'))->getModifiers(), '|'; echo (new ReflectionMethod(ReflectionMetaProbe::class, 'priv'))->getModifiers();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"49|0:1:2|66|4");
}

#[test]
fn reflection_class_exposes_names_parent_interfaces_and_member_counts() {
    let result = execute_source(
        "<?php namespace P21\\Ns; class Base {} interface I {} abstract class Child extends Base implements I { public const C = 1; public string $name; public function run(): void {} } $class = new \\ReflectionClass(Child::class); echo $class->getName(), '|', $class->getShortName(), '|', $class->getNamespaceName(), '|', ($class->inNamespace() ? 'namespace' : 'global'), '|'; echo $class->isAbstract() ? 'abstract|' : 'concrete|'; echo $class->isInterface() ? 'iface|' : 'class|'; echo $class->isEnum() ? 'enum|' : 'notenum|'; echo $class->getParentClass()->getName(), '|', $class->getInterfaceNames()[0], '|'; echo count($class->getMethods()), ':', count($class->getProperties()), ':', count($class->getConstants());",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"P21\\Ns\\Child|Child|P21\\Ns|namespace|abstract|class|notenum|P21\\Ns\\Base|P21\\Ns\\I|1:1:1"
        );
}

#[test]
fn reflection_class_accepts_interface_targets() {
    let result = execute_source(
        "<?php interface ReflectionInterfaceMetadata { public function execute(string $value): string; } $class = new ReflectionClass(ReflectionInterfaceMetadata::class); echo $class->getName(), '|'; echo $class->isInterface() ? 'interface|' : 'class|'; echo $class->isInstantiable() ? 'instantiable|' : 'not-instantiable|'; echo $class->getMethods()[0]->getName();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"ReflectionInterfaceMetadata|interface|not-instantiable|execute"
    );
}

#[test]
fn reflection_extension_lists_enabled_symbols() {
    let result = execute_source(
        "<?php $spl = new ReflectionExtension('spl'); echo $spl->getName(), '|'; $found = 'missing'; foreach ($spl->getClassNames() as $name) { if ($name === 'ArrayObject') { $found = 'arrayobject'; } } echo $found, '|'; $standard = new ReflectionExtension('standard'); $functions = $standard->getFunctions(); echo $functions['count']->getName(), '|', $functions['count']->getExtensionName(), '|'; $reflection = new ReflectionExtension('reflection'); echo $reflection->getName();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"spl|arrayobject|count|standard|Reflection"
    );
}

#[test]
fn reflection_members_expose_property_constant_and_attribute_metadata() {
    let result = execute_source(
        "<?php #[RepeatMe('a'), RepeatMe('b')] class ReflectionStdlib41 { protected const CODE = 42; public readonly string $name; } $class = new ReflectionClass(ReflectionStdlib41::class); $property = $class->getProperty('name'); $constant = $class->getReflectionConstant('CODE'); $attributes = $class->getAttributes(); echo $property->getName(), '|', ($property->hasType() ? $property->getType()->getName() : 'none'), '|', ($property->isReadOnly() ? 'readonly' : 'mutable'), '|', $property->getModifiers(), '|'; echo $constant->getName(), '|', ($constant->isProtected() ? 'protected' : 'not-protected'), '|', $constant->getValue(), '|', ($constant->isEnumCase() ? 'enum' : 'constant'), '|'; echo $attributes[0]->getName(), ':', ($attributes[0]->isRepeated() ? 'repeated' : 'single'), '|', $attributes[1]->getName(), ':', ($attributes[1]->isRepeated() ? 'repeated' : 'single');",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"name|string|readonly|129|CODE|protected|42|constant|RepeatMe:single|RepeatMe:repeated"
    );
}

#[test]
fn reflection_attribute_new_instance_constructs_userland_attribute() {
    let result = execute_source(
        "<?php #[Attribute] class RouteMeta { public string $path; public int $priority; public function __construct(string $path, int $priority = 0) { $this->path = $path; $this->priority = $priority; } } #[RouteMeta('/health', 10)] class ReflectionRouteTarget {} $attribute = (new ReflectionClass(ReflectionRouteTarget::class))->getAttributes()[0]; $instance = $attribute->newInstance(); echo get_class($instance), '|', $instance->path, '|', $instance->priority;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"RouteMeta|/health|10");
}

#[test]
fn reflection_class_new_instance_args_constructs_with_array_arguments() {
    let result = execute_source(
        "<?php class ReflectionNewInstanceArgsProbe { public string $value; public function __construct(string $a, string $b = 'b') { $this->value = $a . $b; } } $class = new ReflectionClass(ReflectionNewInstanceArgsProbe::class); $object = $class->newInstanceArgs(['a', 'c']); echo get_class($object), '|', $object->value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"ReflectionNewInstanceArgsProbe|ac"
    );
}

#[test]
fn reflection_class_new_instance_constructs_with_variadic_arguments() {
    let result = execute_source(
        "<?php class ReflectionNewInstanceProbe { public string $value; public function __construct(string $a, string $b = 'b') { $this->value = $a . $b; } } $class = new ReflectionClass(ReflectionNewInstanceProbe::class); $object = $class->newInstance('x', 'y'); echo get_class($object), '|', $object->value;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ReflectionNewInstanceProbe|xy");
}

#[test]
fn namespaced_instanceof_matches_implemented_interface() {
    let result = execute_source(
        "<?php namespace SimplePie\\Cache; interface NameFilter {} final class CallableNameFilter implements NameFilter {} $filter = new CallableNameFilter(); echo $filter instanceof NameFilter ? 'local|' : 'local-miss|'; echo $filter instanceof \\SimplePie\\Cache\\NameFilter ? 'fqcn' : 'fqcn-miss';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"local|fqcn");
}

#[test]
fn object_property_keeps_namespaced_interface_implementation() {
    let result = execute_source(
        "<?php namespace SimplePie\\Cache; interface NameFilter {} final class CallableNameFilter implements NameFilter {} namespace SimplePie; use SimplePie\\Cache\\CallableNameFilter; use SimplePie\\Cache\\NameFilter; class SimplePie { public $cache_name_function = 'md5'; public $cache_namefilter; public function __construct() { $this->cache_namefilter = new CallableNameFilter(); } public function pass(Sanitize $sanitize): void { $sanitize->pass_cache_data($this->cache_namefilter); } } class Sanitize { public function pass_cache_data($cache_name_function): void { echo get_class($cache_name_function), '|'; echo is_string($cache_name_function) ? 'string|' : 'not-string|'; echo $cache_name_function instanceof \\SimplePie\\Cache\\NameFilter ? 'fqcn|' : 'fqcn-miss|'; echo $cache_name_function instanceof NameFilter ? 'filter' : gettype($cache_name_function); } } $feed = new SimplePie(); $feed->pass(new Sanitize());",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"SimplePie\\Cache\\CallableNameFilter|not-string|fqcn|filter"
    );
}

#[test]
fn reflection_enum_backed_cases_are_distinct_and_queryable() {
    let result = execute_source(
        "<?php enum ReflectionStdlib41Status: string { #[CaseMark('ready')] case Ready = 'ready'; case Done = 'done'; } $enum = new ReflectionEnum(ReflectionStdlib41Status::class); echo $enum->hasCase('Ready') ? 'has|' : 'missing|'; $case = $enum->getCase('Ready'); echo get_class($case), '|', $case->getName(), '|', $case->getBackingValue(), '|'; $direct = new ReflectionEnumBackedCase(ReflectionStdlib41Status::class, 'Done'); echo get_class($direct), '|', $direct->getBackingValue(), '|'; $constant = (new ReflectionClass(ReflectionStdlib41Status::class))->getReflectionConstant('Ready'); echo $constant->isEnumCase() ? 'enumcase' : 'constant';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"has|ReflectionEnumBackedCase|Ready|ready|ReflectionEnumBackedCase|done|enumcase"
    );
}

#[test]
fn tokenizer_token_get_all_exposes_lexer_tokens() {
    let result = execute_source(
        "<?php $tokens = token_get_all('<?php echo $name + 1;'); echo token_name($tokens[0][0]), '|', $tokens[0][1], '|', $tokens[0][2], '|'; foreach ($tokens as $token) { if (is_array($token) && token_name($token[0]) === 'T_VARIABLE') { echo token_name($token[0]), ':', $token[1], '|'; } if ($token === '+') { echo 'symbol:+|'; } } echo token_name(-1);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"T_OPEN_TAG|<?php |1|T_VARIABLE:$name|symbol:+|UNKNOWN"
    );
}

#[test]
fn tokenizer_php_token_objects_support_is_and_ignorable() {
    let result = execute_source(
        "<?php $tokens = PhpToken::tokenize(\"<?php // hi\\n echo 1;\"); echo get_class($tokens[0]), '|', $tokens[0]->getTokenName(), '|', $tokens[0]->line, ':', $tokens[0]->pos, '|'; foreach ($tokens as $token) { if ($token->isIgnorable()) { echo 'I:', $token->getTokenName(), '|'; } if ($token->is(T_ECHO)) { echo 'echo|'; } if ($token->is([';', T_LNUMBER])) { echo 'match:', $token->text, '|'; } }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"PhpToken|T_OPEN_TAG|1:0|I:T_OPEN_TAG|I:T_COMMENT|I:T_WHITESPACE|echo|I:T_WHITESPACE|match:1|match:;|"
        );
}

#[test]
fn tokenizer_php_token_constructor_sets_public_shape_and_stringable() {
    let result = execute_source(
        "<?php $token = new PhpToken(T_FUNCTION, 'function', 10, 100); echo get_class($token), '|', $token->id === T_FUNCTION ? 'id' : 'bad-id', '|', $token->text, '|', $token->line, '|', $token->pos, '|'; echo $token instanceof Stringable ? 'stringable|' : 'not-stringable|'; echo $token->__toString(), '|', $token->getTokenName(), '|'; $unknown = new PhpToken(-1, 'custom'); echo is_null($unknown->getTokenName()) ? 'null' : 'name';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"PhpToken|id|function|10|100|stringable|function|T_FUNCTION|null"
    );
}

#[test]
fn tokenizer_php_token_constructor_reuses_unrooted_call_argument_handles() {
    let result = execute_source(
        "<?php $token = new PhpToken(300, 'function'); var_dump($token); $token = new PhpToken(300, 'function', 10); var_dump($token); $token = new PhpToken(300, 'function', 10, 100); var_dump($token);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"object(PhpToken)#1 (4) {\n  [\"id\"]=>\n  int(300)\n  [\"text\"]=>\n  string(8) \"function\"\n  [\"line\"]=>\n  int(-1)\n  [\"pos\"]=>\n  int(-1)\n}\nobject(PhpToken)#2 (4) {\n  [\"id\"]=>\n  int(300)\n  [\"text\"]=>\n  string(8) \"function\"\n  [\"line\"]=>\n  int(10)\n  [\"pos\"]=>\n  int(-1)\n}\nobject(PhpToken)#1 (4) {\n  [\"id\"]=>\n  int(300)\n  [\"text\"]=>\n  string(8) \"function\"\n  [\"line\"]=>\n  int(10)\n  [\"pos\"]=>\n  int(100)\n}\n"
    );
}

#[test]
fn tokenizer_php_token_casts_to_string_in_runtime_builtins() {
    let result = execute_source(
        "<?php $tokens = PhpToken::tokenize('<?php echo \"Hello \". $what;'); var_dump(implode($tokens)); var_dump((string) $tokens[0]); var_dump($tokens[0]->__toString());",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"string(27) \"<?php echo \"Hello \". $what;\"\nstring(6) \"<?php \"\nstring(6) \"<?php \"\n"
    );
}

#[test]
fn tokenizer_php_token_subclass_tokenize_uses_called_class() {
    let result = execute_source(
        "<?php class MyPhpToken extends PhpToken { public int $extra = 123; public function lowered(): string { return strtolower($this->text); } } $tokens = MyPhpToken::tokenize('<?PHP ECHO 1;'); echo get_class($tokens[0]), '|', $tokens[0] instanceof PhpToken ? 'parent|' : 'no-parent|', $tokens[0]->extra, '|'; foreach ($tokens as $token) { if ($token->text === 'ECHO') { echo $token->lowered(); } }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"MyPhpToken|parent|123|echo");
}

#[test]
fn tokenizer_php_token_subclass_construction_errors_are_catchable() {
    let result = execute_source(
        r#"<?php
        class MyPhpToken1 extends PhpToken {
            public $extra = UNKNOWN;
        }
        try {
            var_dump(MyPhpToken1::tokenize("<?php foo"));
        } catch (Error $e) {
            echo $e->getMessage(), "\n";
        }
        abstract class MyPhpToken2 extends PhpToken {
        }
        try {
            var_dump(MyPhpToken2::tokenize("<?php foo"));
        } catch (Error $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"Undefined constant \"UNKNOWN\"\nCannot instantiate abstract class MyPhpToken2\n"
    );
}

#[test]
fn tokenizer_php_token_constructor_is_final() {
    let result = execute_source(
        "<?php class MyPhpToken extends PhpToken { public function __construct() {} }",
    );

    assert!(matches!(
        first_vm_compile_payload(&result),
        VmCompileDiagnostic::FinalMethodOverride {
            method_name,
            parent_class_name,
            ..
        } if method_name == "__construct" && parent_class_name == "PhpToken"
    ));
}

#[test]
fn closures_execute_simple_calls_captures_arrows_and_returns() {
    let simple = execute_source("<?php $f = function($x) { return $x + 1; }; echo $f(2);");
    assert!(simple.status.is_success(), "{:?}", simple.status);
    assert_eq!(simple.output.as_bytes(), b"3");

    let use_by_value = execute_source(
        "<?php $x = 2; $f = function($y) use ($x) { return $x + $y; }; $x = 100; echo $f(3);",
    );
    assert!(
        use_by_value.status.is_success(),
        "{:?}",
        use_by_value.status
    );
    assert_eq!(use_by_value.output.as_bytes(), b"5");

    let arrow = execute_source("<?php $x = 4; $f = fn($y) => $x + $y; $x = 100; echo $f(3);");
    assert!(arrow.status.is_success(), "{:?}", arrow.status);
    assert_eq!(arrow.output.as_bytes(), b"7");

    let returned = execute_source(
        "<?php function make($x) { return function() use ($x) { return $x; }; } $f = make(9); echo $f();",
    );
    assert!(returned.status.is_success(), "{:?}", returned.status);
    assert_eq!(returned.output.as_bytes(), b"9");
}

#[test]
fn closure_from_callable_wraps_non_static_static_method_error() {
    let result = execute_source(
        "<?php class A { public function method() {} } try { Closure::fromCallable(['A', 'method']); } catch (TypeError $e) { echo $e->getMessage(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"Failed to create closure from callable: non-static method A::method() cannot be called statically"
        );
}

#[test]
fn closures_capture_by_reference_and_static_locals_execute() {
    let by_ref = execute_source(
        "<?php $x = 1; $f = function() use (&$x) { return $x; }; $x = 4; echo $f();",
    );
    assert!(by_ref.status.is_success(), "{:?}", by_ref.status);
    assert_eq!(by_ref.output.as_bytes(), b"4");

    let write_through =
        execute_source("<?php $x = 1; $f = function() use (&$x) { $x = 7; }; $f(); echo $x;");
    assert!(
        write_through.status.is_success(),
        "{:?}",
        write_through.status
    );
    assert_eq!(write_through.output.as_bytes(), b"7");

    let static_local = execute_source(
        "<?php function next_id() { static $x = 0; $x++; return $x; } echo next_id(), '|', next_id();",
    );
    assert!(
        static_local.status.is_success(),
        "{:?}",
        static_local.status
    );
    assert_eq!(static_local.output.as_bytes(), b"1|2");

    let closure_static = execute_source(
        "<?php $f = function() { static $x = 0; $x++; return $x; }; echo $f(), '|', $f();",
    );
    assert!(
        closure_static.status.is_success(),
        "{:?}",
        closure_static.status
    );
    assert_eq!(closure_static.output.as_bytes(), b"1|2");

    let by_ref_return = execute_source(
        "<?php function &counter() { static $x = 0; return $x; } $a =& counter(); $a = 5; echo counter();",
    );
    assert!(
        by_ref_return.status.is_success(),
        "{:?}",
        by_ref_return.status
    );
    assert_eq!(by_ref_return.output.as_bytes(), b"5");
}

#[test]
fn pipe_executes_user_function_closure_builtin_and_non_callable_error() {
    let user_function =
        execute_source("<?php function plus1($x) { return $x + 1; } echo 2 |> plus1(...);");
    assert!(
        user_function.status.is_success(),
        "{:?}",
        user_function.status
    );
    assert_eq!(user_function.output.as_bytes(), b"3");

    let closure = execute_source("<?php $f = fn($x) => $x + 2; echo 2 |> $f;");
    assert!(closure.status.is_success(), "{:?}", closure.status);
    assert_eq!(closure.output.as_bytes(), b"4");

    let builtin = execute_source(
        "<?php echo \" a \" |> trim(...), \"|\"; echo \"ab\" |> strlen(...), \"|\"; echo \"hi\" |> strtoupper(...);",
    );
    assert!(builtin.status.is_success(), "{:?}", builtin.status);
    assert_eq!(builtin.output.as_bytes(), b"a|2|HI");

    let side_effects = execute_source(
        "<?php function id($x) { return $x; } $x = 0; echo ($x = 7) |> id(...); echo \"|\", $x;",
    );
    assert!(
        side_effects.status.is_success(),
        "{:?}",
        side_effects.status
    );
    assert_eq!(side_effects.output.as_bytes(), b"7|7");

    let not_callable =
        execute_source("<?php try { echo 2 |> 4; } catch (Error $e) { echo 'invalid'; }");
    assert!(
        not_callable.status.is_success(),
        "{:?}",
        not_callable.status
    );
    assert_eq!(not_callable.output.as_bytes(), b"invalid");

    let uncaught_not_callable = execute_source("<?php echo 2 |> 4;");
    assert_eq!(
        uncaught_not_callable.status.exit_status(),
        ExitStatus::RuntimeError
    );
    let message = uncaught_not_callable
        .status
        .message()
        .expect("runtime error message");
    assert!(message.contains("Uncaught Error"), "{message}");
}

#[test]
fn arrays_execute_indexed_and_string_key_literals() {
    let result = execute_source(
        "<?php $a = [1, 2, \"x\" => 3]; echo $a[0], \"|\", $a[1], \"|\", $a[\"x\"]; ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2|3");
}

#[test]
fn arrays_execute_append_and_overwrite_assignments() {
    let result = execute_source(
        "<?php $a = []; $a[] = 1; $a[] = 2; $a[1] = 5; $a[\"k\"] = 7; echo $a[0], \"|\", $a[1], \"|\", $a[\"k\"];",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|5|7");
}

#[test]
fn arrays_execute_nested_fetch_and_assignment() {
    let result = execute_source(
        "<?php $a = [\"outer\" => [\"inner\" => 4]]; $a[\"outer\"][\"next\"] = 8; echo $a[\"outer\"][\"inner\"], \"|\", $a[\"outer\"][\"next\"]; ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"4|8");
}

#[test]
fn arrays_execute_keyed_nested_append_assignment() {
    let result = execute_source(
        "<?php $cache = []; $id = 1; $key = 'session_tokens'; $cache[$id] = []; $cache[$id][$key] = []; $cache[$id][$key][] = 'token'; var_export($cache);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array (\n  1 => \n  array (\n    'session_tokens' => \n    array (\n      0 => 'token',\n    ),\n  ),\n)"
    );
}

#[test]
fn null_coalescing_assignment_executes_for_locals_and_dimensions() {
    let result = execute_source(
        "<?php $value ??= 'fallback'; $value ??= 'ignored'; $a = ['path' => 'ok', 'null' => null]; echo $value, '|'; echo ($a['path'] ??= 'bad'), '|'; echo ($a['missing'] ??= 'new'), '|', $a['missing'], '|'; echo ($a['null'] ??= 'filled'), '|', $a['null'];",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"fallback|ok|new|new|filled|filled"
    );
}

#[test]
fn arrays_missing_key_warns_and_reads_null() {
    let result = execute_source("<?php $a = []; echo $a[\"missing\"], \"x\";");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"x");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING"
    );
    assert!(
        result.diagnostics[0].source_span().file.is_some(),
        "{:?}",
        result.diagnostics[0]
    );
    assert!(result.diagnostics[0].source_span().end > result.diagnostics[0].source_span().start);
}

#[test]
fn arrays_execute_isset_empty_and_unset() {
    let result = execute_source(
        "<?php $a = [\"x\" => 0, \"y\" => 1]; echo isset($a[\"x\"]), isset($a[\"z\"]), \"|\"; echo empty($a[\"x\"]), empty($a[\"z\"]), empty($missing), \"|\"; unset($a[\"y\"]); echo isset($a[\"y\"]);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|111|");
}

#[test]
fn arrays_execute_property_isset_chains_quiet_missing_offsets() {
    let result = execute_source(
        r#"<?php
            $a = [];
            $property = "response";
            echo isset($a[0]->response) ? "yes" : "no";
            echo "|", empty($a[0]->response["x"]) ? "empty" : "filled";
            echo "|", isset($a[0]->$property) ? "dyn" : "dyn-no";
            echo "|", empty($a[0]->$property["x"]) ? "dyn-empty" : "dyn-filled";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"no|empty|dyn-no|dyn-empty");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn userland_arrayaccess_routes_offset_operations() {
    let result = execute_source(
        r#"<?php
            class Box implements ArrayAccess {
                public array $items = [];
                public array $log = [];
                public function offsetExists($offset): bool {
                    $this->log[] = "exists:" . (string) $offset;
                    return array_key_exists($offset, $this->items);
                }
                public function offsetGet($offset): mixed {
                    $this->log[] = "get:" . (string) $offset;
                    return $this->items[$offset] ?? null;
                }
                public function offsetSet($offset, $value): void {
                    $this->log[] = "set:" . ($offset === null ? "null" : (string) $offset) . "=" . (string) $value;
                    if ($offset === null) {
                        $this->items[] = $value;
                    } else {
                        $this->items[$offset] = $value;
                    }
                }
                public function offsetUnset($offset): void {
                    $this->log[] = "unset:" . (string) $offset;
                    unset($this->items[$offset]);
                }
            }
            $box = new Box();
            $box["a"] = 0;
            $box[] = 2;
            echo $box["a"], "|", $box[0], "|";
            echo isset($box["a"]) ? "isset|" : "missing|";
            echo empty($box["a"]) ? "empty|" : "filled|";
            unset($box["a"]);
            echo isset($box["a"]) ? "bad|" : "gone|";
            foreach ($box->log as $entry) {
                echo $entry, ";";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"0|2|isset|empty|gone|set:a=0;set:null=2;get:a;get:0;exists:a;exists:a;get:a;unset:a;exists:a;"
        );
}

#[test]
fn userland_arrayaccess_supports_nested_isset_and_empty() {
    let result = execute_source(
        r#"<?php
            class Box implements ArrayAccess {
                public array $items = [];
                public array $log = [];
                public function offsetExists($offset): bool {
                    $this->log[] = "exists:" . (string) $offset;
                    return array_key_exists($offset, $this->items);
                }
                public function offsetGet($offset): mixed {
                    $this->log[] = "get:" . (string) $offset;
                    return $this->items[$offset] ?? null;
                }
                public function offsetSet($offset, $value): void {
                    $this->items[$offset] = $value;
                }
                public function offsetUnset($offset): void {
                    unset($this->items[$offset]);
                }
            }
            $box = new Box();
            $box["route"] = ["status" => "ok", "zero" => 0, "null" => null];
            echo isset($box["route"]["status"]) ? "yes" : "no";
            echo "|", isset($box["route"]["null"]) ? "bad" : "null";
            echo "|", empty($box["route"]["zero"]) ? "empty" : "filled";
            echo "|", empty($box["missing"]["status"]) ? "missing-empty" : "bad";
            echo "|", implode(",", $box->log);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"yes|null|empty|missing-empty|exists:route,get:route,exists:route,get:route,exists:route,get:route,exists:missing"
        );
}

#[test]
fn userland_arrayaccess_supports_nested_arrayaccess_child_isset_and_empty() {
    let result = execute_source(
        r#"<?php
            class Box implements ArrayAccess {
                public array $items = [];
                public array $log = [];
                public string $name;
                public function __construct(string $name) { $this->name = $name; }
                public function offsetExists($offset): bool {
                    $this->log[] = $this->name . ":exists:" . (string) $offset;
                    return array_key_exists($offset, $this->items);
                }
                public function offsetGet($offset): mixed {
                    $this->log[] = $this->name . ":get:" . (string) $offset;
                    return $this->items[$offset] ?? null;
                }
                public function offsetSet($offset, $value): void {
                    $this->items[$offset] = $value;
                }
                public function offsetUnset($offset): void {
                    unset($this->items[$offset]);
                }
            }
            $outer = new Box("outer");
            $inner = new Box("inner");
            $inner["flag"] = "ok";
            $inner["zero"] = 0;
            $outer["child"] = $inner;
            echo isset($outer["child"]["flag"]) ? "yes" : "no";
            echo "|", empty($outer["child"]["zero"]) ? "empty" : "filled";
            echo "|", implode(",", $outer->log), "|", implode(",", $inner->log);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"yes|empty|outer:exists:child,outer:get:child,outer:exists:child,outer:get:child|inner:exists:flag,inner:exists:zero,inner:get:zero"
        );
}

#[test]
fn userland_arrayaccess_routes_property_dimension_assignment() {
    let result = execute_source(
        r#"<?php
            class Headers implements ArrayAccess {
                public array $items = [];
                public array $log = [];
                public function offsetExists($offset): bool {
                    return array_key_exists(strtolower((string) $offset), $this->items);
                }
                public function offsetGet($offset): mixed {
                    $this->log[] = "get:" . (string) $offset;
                    return $this->items[strtolower((string) $offset)] ?? null;
                }
                public function offsetSet($offset, $value): void {
                    $this->log[] = "set:" . (string) $offset . "=" . (string) $value;
                    $this->items[strtolower((string) $offset)] = $value;
                }
                public function offsetUnset($offset): void {
                    unset($this->items[strtolower((string) $offset)]);
                }
            }
            class Response {
                public Headers $headers;
                public function __construct() {
                    $this->headers = new Headers();
                }
            }
            $response = new Response();
            $response->headers["Content-Type"] = "text/html";
            echo $response->headers["content-type"], "|", $response->headers->log[0];
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"text/html|set:Content-Type=text/html"
    );
}

#[test]
fn exceptions_catch_exception_object() {
    let result = execute_source(
        "<?php try { throw new Exception(\"boom\"); } catch (Exception $e) { echo \"caught:\", $e->getMessage(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"caught:boom");
}

#[test]
fn exceptions_catch_userland_exception_thrown_below_static_method() {
    let result = execute_source(
        r#"<?php
            namespace WpOrg\Requests {
                class Exception extends \Exception {
                    protected $type;
                    protected $data;

                    public function __construct($message, $type, $data = null, $code = 0) {
                        parent::__construct($message, $code);
                        $this->type = $type;
                        $this->data = $data;
                    }
                }

                class Requests {
                    public static function request() {
                        $transport = new \WpOrg\Requests\Transport\Curl();
                        return $transport->request();
                    }
                }
            }

            namespace WpOrg\Requests\Transport {
                use WpOrg\Requests\Exception;

                final class Curl {
                    public function request() {
                        $this->process_response();
                    }

                    public function process_response() {
                        throw new Exception('cURL error 35: wrong version number', 'curlerror', $this);
                    }
                }
            }

            namespace {
                try {
                    \WpOrg\Requests\Requests::request();
                } catch (WpOrg\Requests\Exception $e) {
                    echo 'caught:', $e->getMessage();
                }
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"caught:cURL error 35: wrong version number"
    );
}

#[test]
fn exceptions_catch_autoloaded_userland_exception_subclass() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-autoload-catch-exception-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let http_dir = root.join("SimplePie/HTTP");
    std::fs::create_dir_all(&http_dir).expect("autoload fixture dirs should be created");
    std::fs::write(
        root.join("SimplePie/Exception.php"),
        "<?php namespace SimplePie; class Exception extends \\Exception {}",
    )
    .expect("base exception should be written");
    std::fs::write(
            http_dir.join("ClientException.php"),
            "<?php namespace SimplePie\\HTTP; use SimplePie\\Exception as SimplePieException; final class ClientException extends SimplePieException {}",
        )
        .expect("client exception should be written");
    std::fs::write(
            http_dir.join("FileClient.php"),
            "<?php namespace SimplePie\\HTTP; final class FileClient { public function request(): void { throw new ClientException('feed failed'); } }",
        )
        .expect("file client should be written");
    let source = r#"<?php
            use SimplePie\HTTP\ClientException;
            spl_autoload_register(function ($class) {
                if ($class === 'SimplePie\HTTP\FileClient') {
                    require __DIR__ . '/SimplePie/HTTP/FileClient.php';
                } elseif ($class === 'SimplePie\HTTP\ClientException') {
                    require __DIR__ . '/SimplePie/HTTP/ClientException.php';
                } elseif ($class === 'SimplePie\Exception') {
                    require __DIR__ . '/SimplePie/Exception.php';
                }
            });
            try {
                (new SimplePie\HTTP\FileClient())->request();
            } catch (ClientException $e) {
                echo 'caught:', $e->getMessage();
            }
        "#;
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"caught:");
}

#[test]
fn exceptions_run_finally_before_return() {
    let result = execute_source(
        "<?php function f() { try { return \"body\"; } finally { echo \"finally|\"; } } echo f();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"finally|body");
}

#[test]
fn exceptions_run_finally_before_uncaught_throw() {
    let result = execute_source(
        "<?php try { throw new Exception(\"boom\"); } finally { echo \"finally\"; }",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_uncaught_exception_output_prefix(
        &result.output.to_string_lossy(),
        "finally",
        "Exception",
        "boom",
    );
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
}

#[test]
fn exceptions_rethrow_from_catch_is_uncaught() {
    let result = execute_source(
        "<?php try { throw new Exception(\"boom\"); } catch (Exception $e) { echo \"catch|\"; throw $e; }",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_uncaught_exception_output_prefix(
        &result.output.to_string_lossy(),
        "catch|",
        "Exception",
        "boom",
    );
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
}

#[test]
fn exceptions_catch_throwable_interface() {
    let result = execute_source(
        "<?php try { throw new Exception(\"boom\"); } catch (Throwable $e) { echo \"throwable:\", $e->getMessage(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"throwable:boom");
}

#[test]
fn exceptions_catch_error_parent_for_type_error() {
    let result = execute_source(
        "<?php try { throw new TypeError(\"bad\"); } catch (Error $e) { echo \"error:\", $e->getMessage(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"error:bad");
}

#[test]
fn exceptions_support_spl_logic_and_runtime_hierarchy() {
    let result = execute_source(
        "<?php
            try {
                throw new InvalidArgumentException('bad');
            } catch (LogicException $e) {
                echo 'logic:', $e->getMessage(), '|';
            }
            $runtime = new UnexpectedValueException('runtime');
            echo ($runtime instanceof RuntimeException) ? 'runtime|' : 'no|';
            echo ($runtime instanceof Exception) ? 'exception|' : 'no|';
            echo ($runtime instanceof LogicException) ? 'logic' : 'not-logic';
            echo '|';
            $json = new JsonException('json');
            echo ($json instanceof Exception) ? 'json-exception' : 'json-no';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"logic:bad|runtime|exception|not-logic|json-exception"
    );
}

#[test]
fn exceptions_skip_nonmatching_catch_and_run_finally() {
    let result = execute_source(
        "<?php try { try { throw new TypeError(\"bad\"); } catch (Exception $e) { echo \"wrong\"; } finally { echo \"finally|\"; } } catch (Throwable $e) { echo \"outer:\", $e->getMessage(); }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"finally|outer:bad");
}

#[test]
fn exceptions_internal_throwable_hierarchy_supports_instanceof() {
    let result = execute_source(
        "<?php $e = new TypeError(\"bad\"); echo ($e instanceof Throwable) ? \"throwable|\" : \"no|\"; echo ($e instanceof Error) ? \"error|\" : \"no|\"; echo ($e instanceof Exception) ? \"exception\" : \"not-exception\";",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"throwable|error|not-exception");
}

#[test]
fn foreach_executes_value_iteration() {
    let result = execute_source("<?php foreach ([1, 2, 3] as $value) { echo $value; }");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"123");
}

#[test]
fn foreach_executes_key_value_iteration_in_insertion_order() {
    let result = execute_source(
        "<?php foreach ([\"a\" => 1, 4 => 2, \"b\" => 3] as $key => $value) { echo $key, \":\", $value, \";\"; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a:1;4:2;b:3;");
}

#[test]
fn spl_array_iterator_methods_and_foreach_preserve_keys_values() {
    let result = execute_source(
        r#"<?php
            $it = new ArrayIterator(["a" => 10, "b" => 20]);
            echo $it->key(), "=", $it->current(), "|";
            $it->next();
            echo $it->key(), "=", $it->current(), "|";
            echo $it->valid() ? "valid|" : "invalid|";
            $it->rewind();
            foreach ($it as $key => $value) {
                echo "f:", $key, "=", $value, "|";
            }
            echo $it->count(), "|", count($it);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"a=10|b=20|valid|f:a=10|f:b=20|2|2"
    );
}

#[test]
fn spl_array_iterator_rejects_dimension_assignment_by_reference() {
    let result = execute_source(
        r#"<?php
            $tmp = 1;
            $it = new ArrayIterator();
            $it[] = $tmp;
            $it[] = &$tmp;
            echo "unreachable";
            "#,
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with(
            "Notice: Indirect modification of overloaded element of ArrayIterator has no effect in "
        ),
        "{output}"
    );
    assert!(
            output.contains(
                "Fatal error: Uncaught Error: Cannot assign by reference to an array dimension of an object"
            ),
            "{output}"
        );
    assert!(!output.contains("unreachable"), "{output}");
}

#[test]
fn spl_iterator_functions_cover_arrays_and_array_iterator_mvp() {
    let result = execute_source(
        r#"<?php
            echo iterator_count([]), "|", iterator_count(["a" => 1, "b" => 2]), "|";
            var_dump(iterator_to_array(["a" => 1, "b" => 2, 5 => 3]));
            var_dump(iterator_to_array(["a" => 1, "b" => 2, 5 => 3], false));
            $it = new ArrayIterator(["x" => 7, "y" => 8]);
            echo iterator_count($it), "|";
            var_dump(iterator_to_array($it));
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "0|2|array(3) {\n  [\"a\"]=>\n  int(1)\n  [\"b\"]=>\n  int(2)\n  [5]=>\n  int(3)\n}\narray(3) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n  [2]=>\n  int(3)\n}\n2|array(2) {\n  [\"x\"]=>\n  int(7)\n  [\"y\"]=>\n  int(8)\n}\n"
    );
}

#[test]
fn spl_iterator_functions_reject_non_iterables_with_type_error() {
    let result = execute_source(
        r#"<?php
            try {
                iterator_count("1");
            } catch (TypeError $e) {
                echo $e->getMessage(), "|";
            }
            try {
                iterator_to_array("test", "test");
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, string given|iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, string given"
        );
}

#[test]
fn spl_iterator_count_generator_exception_trace_uses_internal_frame() {
    let result = execute_source(
        r#"<?php
            function generator() {
                yield 1;
                throw new Exception('Iterator failed');
            }
            var_dump(iterator_count(generator()));
            "#,
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Fatal error: Uncaught Exception: Iterator failed in "),
        "{output}"
    );
    assert!(
        output.contains("Stack trace:\n#0 [internal function]: generator()\n#1 "),
        "{output}"
    );
    assert!(
        output.contains(": iterator_count(Object(Generator))\n#2 {main}"),
        "{output}"
    );
}

#[test]
fn spl_limit_iterator_rejects_invalid_bounds() {
    let result = execute_source(
        r#"<?php
            $it = new ArrayIterator([1, 2, 3]);
            try {
                new LimitIterator($it, -1);
            } catch (ValueError $e) {
                echo $e->getMessage(), "|";
            }
            try {
                new LimitIterator($it, 0, -2);
            } catch (ValueError $e) {
                echo $e->getMessage(), "|";
            }
            new LimitIterator($it, 0, -1);
            echo "ok";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"LimitIterator::__construct(): Argument #2 ($offset) must be greater than or equal to 0|LimitIterator::__construct(): Argument #3 ($limit) must be greater than or equal to -1|ok"
        );
}

#[test]
fn spl_limit_iterator_get_position_and_seek_use_absolute_offsets() {
    let result = execute_source(
        r#"<?php
            $it = new LimitIterator(new ArrayIterator([1, 2, 3, 4]), 1, 2);
            foreach ($it as $key => $value) {
                echo $key, "=", $value, ":", $it->getPosition(), "|";
            }
            try {
                $it->seek(0);
            } catch (OutOfBoundsException $e) {
                echo $e->getMessage(), "|";
            }
            $it->seek(2);
            echo $it->current(), "|";
            try {
                $it->seek(3);
            } catch (OutOfBoundsException $e) {
                echo $e->getMessage(), "|";
            }
            $it->next();
            echo $it->valid() ? "valid" : "invalid";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"1=2:1|2=3:2|Cannot seek to 0 which is below the offset 1|3|Cannot seek to 3 which is behind offset 1 plus count 2|invalid"
        );
}

#[test]
fn spl_limit_iterator_rejects_offsets_beyond_source_length() {
    let result = execute_source(
        r#"<?php
            try {
                new LimitIterator(new ArrayIterator([1, 2, 3]), PHP_INT_MAX, PHP_INT_MAX);
            } catch (OutOfBoundsException $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"Seek position 9223372036854775807 is out of range"
    );
}

#[test]
fn spl_iterator_diagnostics_reject_invalid_modes_and_flags() {
    let result = execute_source(
        r#"<?php
            $it = new ArrayIterator(["foo"]);
            $regex = new RegexIterator($it, "/foo/");
            try {
                $regex->setMode(7);
            } catch (ValueError $e) {
                echo $e->getMessage(), "|";
            }
            try {
                new CachingIterator($it, CachingIterator::CALL_TOSTRING | CachingIterator::TOSTRING_USE_KEY);
            } catch (ValueError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"RegexIterator::setMode(): Argument #1 ($mode) must be RegexIterator::MATCH, RegexIterator::GET_MATCH, RegexIterator::ALL_MATCHES, RegexIterator::SPLIT, or RegexIterator::REPLACE|CachingIterator::__construct(): Argument #2 ($flags) must contain only one of CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, CachingIterator::TOSTRING_USE_CURRENT, or CachingIterator::TOSTRING_USE_INNER"
        );
}

#[test]
fn spl_caching_iterator_cache_access_requires_full_cache() {
    let result = execute_source(
        r#"<?php
            $plain = new CachingIterator(new ArrayIterator([1, 2]));
            try {
                $plain->count();
            } catch (BadMethodCallException $e) {
                echo "count|";
            }
            try {
                $plain->getCache();
            } catch (BadMethodCallException $e) {
                echo "cache|";
            }
            $full = new CachingIterator(new ArrayIterator([1, 2]), CachingIterator::FULL_CACHE);
            echo $full->count(), "|", count($full->getCache()), "|";
            foreach ($full as $value) {
                echo $full->count(), ":", count($full->getCache()), "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"count|cache|0|0|1:1|2:2|");
}

#[test]
fn spl_caching_iterator_to_string_modes_and_flag_errors() {
    let result = execute_source(
        r#"<?php
            class MyItem {
                function __construct(public $value) {}
                function __toString() { return (string) $this->value; }
            }
            class MyArrayIterator extends ArrayIterator {
                function __toString() { return $this->key() . ':' . $this->current(); }
            }
            foreach ([CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, CachingIterator::TOSTRING_USE_CURRENT] as $flag) {
                $it = new CachingIterator(new ArrayIterator([1, 2]), 0);
                $it->setFlags($flag);
                foreach ($it as $value) {
                    echo (string) $it, '|';
                }
            }
            $it = new CachingIterator(new MyArrayIterator([new MyItem(1), new MyItem(2)]), 0);
            $it->setFlags(CachingIterator::TOSTRING_USE_INNER);
            foreach ($it as $value) {
                echo (string) $it, '|';
            }
            $it = new CachingIterator(new ArrayIterator([1]), 0);
            try {
                $it->setFlags(CachingIterator::CALL_TOSTRING | CachingIterator::TOSTRING_USE_KEY);
            } catch (ValueError $e) {
                echo $it->getFlags(), '|';
            }
            $it = new CachingIterator(new ArrayIterator([1]), CachingIterator::CALL_TOSTRING);
            try {
                $it->setFlags(0);
            } catch (BadMethodCallException $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"1|2|0|1|1|2|0:1|1:2|0|Unsetting flag CALL_TO_STRING is not possible"
    );
}

#[test]
fn spl_caching_iterator_string_cast_fatal_trace_matches_php_shape() {
    let result = execute_source(
        r#"<?php
            function test($it) {
                foreach ($it as $value) {
                    var_dump((string) $it);
                }
            }
            test(new CachingIterator(new ArrayIterator([1, 2, 3]), 0));
            "#,
    );

    assert!(!result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains(
            "Fatal error: Uncaught BadMethodCallException: CachingIterator does not fetch string value (see CachingIterator::__construct) in "
        ),
        "{output}"
    );
    assert!(
        output.contains(": CachingIterator->__toString()"),
        "{output}"
    );
    assert!(
        output.contains(": test(Object(CachingIterator))"),
        "{output}"
    );
    assert!(
        output.contains("  thrown in /tmp/phrust-test.php on line "),
        "{output}"
    );
}

#[test]
fn spl_recursive_tree_iterator_rejects_non_recursive_iterator_source() {
    let result = execute_source(
        r#"<?php
            try {
                new RecursiveTreeIterator(new ArrayIterator([]));
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"RecursiveCachingIterator::__construct(): Argument #1 ($iterator) must be of type RecursiveIterator, ArrayIterator given"
        );
}

#[test]
fn spl_subclass_constructor_arity_errors_are_type_errors() {
    let result = execute_source(
        r#"<?php
            class MyFilterIterator extends FilterIterator {
                function accept(): bool { return true; }
            }
            try {
                new MyFilterIterator();
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"FilterIterator::__construct() expects exactly 1 argument, 0 given"
    );
}

#[test]
fn spl_iterator_apply_calls_callback_args_and_stops_on_false() {
    let result = execute_source(
        r#"<?php
            function apply_tick($prefix) {
                static $count = 0;
                echo $prefix, ":", $count, "|";
                if ($count == 1) {
                    $count = 2;
                    return false;
                }
                $count = 1;
                return true;
            }
            function apply_true() {
                return true;
            }
            $it = new ArrayIterator(["a" => 1, "b" => 2, "c" => 3]);
            echo iterator_apply($it, "apply_tick", ["seen"]), "|";
            echo iterator_apply($it, "apply_true");
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"seen:0|seen:1|2|3");
}

#[test]
fn spl_iterator_apply_errors_are_catchable_and_falsey_stops() {
    let result = execute_source(
        r#"<?php
            function returns_null() {}
            $it = new ArrayIterator([1, 2, 3]);
            echo iterator_apply($it, "returns_null"), "|";
            try {
                iterator_apply($it, "returns_null", "bad");
            } catch (TypeError $e) {
                echo "type|";
            }
            try {
                iterator_apply($it, "returns_null", null, 4);
            } catch (TypeError $e) {
                echo "arity|";
            }
            try {
                iterator_apply($it, "missing_apply_callback");
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"1|type|arity|iterator_apply(): Argument #2 ($callback) must be a valid callback, function \"missing_apply_callback\" not found or invalid function name"
        );
}

#[test]
fn spl_iterator_apply_honors_subclass_rewind_exception() {
    let result = execute_source(
        r#"<?php
            class ThrowingArrayIterator extends ArrayIterator {
                public function rewind(): void {
                    throw new Exception("Make the iterator break");
                }
            }
            function no_op_apply() {}
            try {
                iterator_apply(new ThrowingArrayIterator([1]), "no_op_apply");
            } catch (Exception $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"Make the iterator break");
}

#[test]
fn spl_iterator_to_array_preserved_keys_match_php_key_diagnostics() {
    let result = execute_source(
        r#"<?php
            function nonscalar_key_generator() {
                yield "foo" => 1;
                yield 1 => 2;
                yield 2.5 => 3;
                yield null => 4;
                yield [] => 5;
            }
            try {
                iterator_to_array(nonscalar_key_generator());
            } catch (Error $e) {
                echo "caught:", $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Implicit conversion from float 2.5 to int loses precision"));
    assert!(
        output.contains("Using null as an array offset is deprecated, use an empty string instead")
    );
    assert!(output.contains("caught:Cannot access offset of type array on array"));
}

#[test]
fn constructor_indirect_temporaries_do_not_reserve_visible_object_ids() {
    let result = execute_source(
        r#"<?php
            class MyFoo {}
            class MyCachingIterator extends CachingIterator {
                function __construct(Iterator $it, $flags = 0) {
                    parent::__construct($it, $flags);
                }
            }
            $it = new MyCachingIterator(new ArrayIterator([0, "foo" => 1, 2, "bar" => 3, 4]));
            try {
                $it->offsetGet(0);
            } catch (Exception $e) {
            }
            $it = new MyCachingIterator(
                new ArrayIterator([0, "foo" => 1, 2, "bar" => 3, 4]),
                CachingIterator::FULL_CACHE
            );
            $checks = [0, new stdClass, new MyFoo, null, 2, "foo", 3];
            echo spl_object_id($checks[1]), "|", spl_object_id($checks[2]);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert_eq!(output, "1|2");
}

#[test]
fn dynamic_iterator_function_releases_temporary_before_return_observed() {
    let result = execute_source(
        r#"<?php
            class DestructingArrayIterator extends ArrayIterator {
                public function __construct() {
                    parent::__construct([1, 2]);
                }
                public function __destruct() {
                    echo "destruct\n";
                }
            }
            $func = 'iterator_to_array';
            var_dump($func(new DestructingArrayIterator()));
            $func = 'iterator_count';
            var_dump($func(new DestructingArrayIterator()));
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "destruct\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\ndestruct\nint(2)\n"
    );
}

#[test]
fn spl_iterator_function_temporary_destructor_exception_is_catchable() {
    let result = execute_source(
        r#"<?php
            class MyArrayIterator extends ArrayIterator {
                static protected $fail = 0;

                static function fail($state, $method) {
                    if (self::$fail == $state) {
                        throw new Exception("State $state: $method()");
                    }
                }

                function __construct() {
                    self::fail(0, __FUNCTION__);
                    parent::__construct([1, 2]);
                    self::fail(1, __FUNCTION__);
                }

                function rewind(): void {
                    self::fail(2, __FUNCTION__);
                    parent::rewind();
                }

                function valid(): bool {
                    self::fail(3, __FUNCTION__);
                    return parent::valid();
                }

                function current(): mixed {
                    self::fail(4, __FUNCTION__);
                    return parent::current();
                }

                function key(): string|int|null {
                    self::fail(5, __FUNCTION__);
                    return parent::key();
                }

                function next(): void {
                    self::fail(6, __FUNCTION__);
                    parent::next();
                }

                function __destruct() {
                    self::fail(7, __FUNCTION__);
                }

                static function test($func, $skip = null) {
                    echo "===$func===\n";
                    self::$fail = 0;
                    while (self::$fail < 10) {
                        try {
                            var_dump($func(new MyArrayIterator()));
                            break;
                        } catch (Exception $e) {
                            echo $e->getMessage(), "\n";
                        }
                        if (isset($skip[self::$fail])) {
                            self::$fail = $skip[self::$fail];
                        } else {
                            self::$fail++;
                        }
                        try {
                            $e = null;
                        } catch (Exception $e) {
                        }
                    }
                }
            }

            MyArrayIterator::test('iterator_to_array');
            MyArrayIterator::test('iterator_count', [3 => 6]);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "===iterator_to_array===\nState 0: __construct()\nState 1: __construct()\nState 2: rewind()\nState 3: valid()\nState 4: current()\nState 5: key()\nState 6: next()\nState 7: __destruct()\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n===iterator_count===\nState 0: __construct()\nState 1: __construct()\nState 2: rewind()\nState 3: valid()\nState 6: next()\nState 7: __destruct()\nint(2)\n"
    );
}

#[test]
fn spl_iterator_wrappers_limit_empty_and_append_iterate() {
    let result = execute_source(
        r#"<?php
            $base = new ArrayIterator(["a" => 1, "b" => 2, "c" => 3]);
            foreach (new IteratorIterator($base) as $key => $value) {
                echo "i:", $key, "=", $value, "|";
            }
            foreach (new LimitIterator(new ArrayIterator([10, 20, 30]), 1, 1) as $key => $value) {
                echo "l:", $key, "=", $value, "|";
            }
            foreach (new EmptyIterator() as $value) {
                echo "bad";
            }
            $append = new AppendIterator();
            $append->append(new ArrayIterator(["x" => 7]));
            $append->append(new ArrayIterator(["y" => 8]));
            foreach ($append as $key => $value) {
                echo "a:", $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"i:a=1|i:b=2|i:c=3|l:1=20|a:x=7|a:y=8|"
    );
}

#[test]
fn spl_empty_iterator_key_and_current_throw_bad_method_call() {
    let result = execute_source(
        r#"<?php
            $it = new EmptyIterator();
            echo $it->valid() ? "bad|" : "invalid|";
            try {
                $it->key();
            } catch (BadMethodCallException $e) {
                echo $e->getMessage(), "|";
            }
            try {
                $it->current();
            } catch (BadMethodCallException $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"invalid|Accessing the key of an EmptyIterator|Accessing the value of an EmptyIterator"
    );
}

#[test]
fn spl_no_rewind_iterator_rewind_keeps_current_position() {
    let result = execute_source(
        r#"<?php
            $it = new NoRewindIterator(new ArrayIterator(["A", "B", "C"]));
            echo $it->key(), "=>", $it->current(), "|";
            $it->next();
            foreach ($it as $key => $value) {
                echo $key, "=>", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0=>A|1=>B|2=>C|");
}

#[test]
fn spl_recursive_array_iterator_and_type_checks_use_internal_metadata() {
    let result = execute_source(
        r#"<?php
            $it = new RecursiveArrayIterator(["k" => "v"]);
            echo ($it instanceof RecursiveArrayIterator) ? "recursive|" : "no|";
            echo ($it instanceof ArrayIterator) ? "array|" : "no|";
            echo ($it instanceof Iterator) ? "iterator|" : "no|";
            echo ($it instanceof Traversable) ? "traversable|" : "no|";
            echo ($it instanceof Countable) ? "countable|" : "no|";
            foreach ($it as $key => $value) {
                echo $key, "=", $value;
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"recursive|array|iterator|traversable|countable|k=v"
    );
}

#[test]
fn spl_recursive_array_iterator_child_can_be_reconstructed_after_parent_drop() {
    let result = execute_source(
        r#"<?php
            $it = new RecursiveArrayIterator([[1]]);
            $child = $it->getChildren();
            unset($it);
            $child->__construct([2, 3]);
            foreach ($child->getArrayCopy() as $value) {
                echo $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2|3|");
}

#[test]
fn spl_internal_iterator_subclass_uses_parent_storage_and_methods() {
    let result = execute_source(
        r#"<?php
            class MyArrayIterator extends ArrayIterator {}
            $it = new MyArrayIterator(["a" => 1, "b" => 2]);
            echo ($it instanceof MyArrayIterator) ? "self|" : "no|";
            echo ($it instanceof ArrayIterator) ? "array|": "no|";
            echo ($it instanceof Iterator) ? "iterator|" : "no|";
            echo ($it instanceof Countable) ? "countable|" : "no|";
            echo $it->count(), "|", count($it), "|";
            foreach ($it as $key => $value) {
                echo $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"self|array|iterator|countable|2|2|a=1|b=2|"
    );
}

#[test]
fn spl_internal_iterator_parent_methods_satisfy_recursive_iterator_contract() {
    let result = execute_source(
        r#"<?php
            class MyRecursiveIterator extends ArrayIterator implements RecursiveIterator {
                public function hasChildren(): bool {
                    return is_array($this->current());
                }

                public function getChildren(): MyRecursiveIterator {
                    return new MyRecursiveIterator($this->current());
                }
            }

            $it = new MyRecursiveIterator([1, [21, 22], 3]);
            echo ($it instanceof RecursiveIterator) ? "recursive|" : "no|";
            foreach ($it as $key => $value) {
                echo $key, "=", is_array($value) ? "array" : $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"recursive|0=1|1=array|2=3|");
}

#[test]
fn spl_internal_iterator_subclass_can_call_parent_methods() {
    let result = execute_source(
        r#"<?php
            class MyEmptyIterator extends EmptyIterator {
                public function rewind(): void {
                    echo __METHOD__, "|";
                    parent::rewind();
                }

                public function valid(): false {
                    echo __METHOD__, "|";
                    return parent::valid();
                }
            }

            foreach (new MyEmptyIterator() as $value) {
                echo "bad";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"MyEmptyIterator::rewind|MyEmptyIterator::valid|"
    );
}

#[test]
fn spl_internal_container_subclass_uses_parent_storage_and_array_access() {
    let result = execute_source(
        r#"<?php
            class MyArrayObject extends ArrayObject {}
            $object = new MyArrayObject(["x" => 7]);
            $object["y"] = 8;
            echo ($object instanceof MyArrayObject) ? "self|" : "no|";
            echo ($object instanceof ArrayObject) ? "arrayobject|" : "no|";
            echo ($object instanceof ArrayAccess) ? "arrayaccess|" : "no|";
            echo count($object), "|", $object["x"], "|", $object["y"], "|";
            foreach ($object as $key => $value) {
                echo $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"self|arrayobject|arrayaccess|2|7|8|x=7|y=8|"
    );
}

#[test]
fn spl_array_object_and_iterator_expose_flag_constants() {
    let result = execute_source(
        r#"<?php
            echo ArrayObject::STD_PROP_LIST, "|", ArrayObject::ARRAY_AS_PROPS, "|";
            echo ArrayIterator::STD_PROP_LIST, "|", ArrayIterator::ARRAY_AS_PROPS, "|";
            echo constant("ArrayObject::ARRAY_AS_PROPS");
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2|1|2|2");
}

#[test]
fn spl_array_object_array_as_props_reads_and_writes_offsets() {
    let result = execute_source(
        r#"<?php
            $object = new ArrayObject(["abc" => 1]);
            $object->setFlags(ArrayObject::ARRAY_AS_PROPS);
            $field = "abc";
            $object->$field++;
            echo $object->$field, "|", $object->abc, "|", $object["abc"], "|";
            $object->abc = 4;
            echo $object["abc"], "|", $object->getFlags();
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2|2|2|4|2");
}

#[test]
fn spl_array_access_empty_uses_userland_offset_get_override() {
    let result = execute_source(
        r#"<?php
            class MyArrayObjectForEmpty extends ArrayObject {
                public function offsetGet($offset): mixed {
                    return [1];
                }
            }
            $object = new MyArrayObjectForEmpty(["qux" => 1]);
            echo empty($object["qux"]) ? "empty" : "filled";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"filled");
}

#[test]
fn spl_iterator_wrapper_unknown_method_stays_on_wrapper() {
    let result = execute_source(
        r#"<?php
            $it = new CachingIterator(new ArrayIterator([1]), CachingIterator::FULL_CACHE);
            $it->doesnotexist("x");
            "#,
    );

    assert!(!result.status.is_success());
    let diagnostic = result
        .diagnostics
        .first()
        .map(|diagnostic| diagnostic.message())
        .unwrap_or_default();
    assert!(
        diagnostic.contains("CachingIterator::doesnotexist"),
        "{diagnostic}"
    );
    assert!(
        !diagnostic.contains("ArrayIterator::doesnotexist")
            && !diagnostic.contains("arrayiterator::doesnotexist"),
        "{diagnostic}"
    );
}

#[test]
fn spl_internal_subclass_parent_constructor_initializes_storage() {
    let result = execute_source(
        r#"<?php
            class MyConstructedIterator extends ArrayIterator {
                public function __construct(array $items) {
                    parent::__construct($items);
                }
            }
            class MyConstructedObject extends ArrayObject {
                public function __construct(array $items) {
                    parent::__construct($items);
                }
            }
            $it = new MyConstructedIterator(["i" => 3]);
            $object = new MyConstructedObject(["o" => 4]);
            echo $it->count(), "|", $it["i"], "|", count($object), "|", $object["o"];
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|3|1|4");
}

#[test]
fn spl_internal_subclass_parent_runtime_methods_use_spl_storage() {
    let result = execute_source(
        r#"<?php
            class ParentCallingIterator extends ArrayIterator {
                public function rewind(): void {
                    echo "rewind|";
                    parent::rewind();
                }
                public function valid(): bool {
                    return parent::valid();
                }
            }
            class ParentCallingObject extends ArrayObject {
                public function __construct(array $items) {
                    parent::__construct($items, ArrayObject::ARRAY_AS_PROPS);
                }
            }
            class ParentCallingHeap extends SplMaxHeap {
                public function compare($a, $b): int {
                    return parent::compare($a, $b);
                }
            }
            $it = new ParentCallingIterator([7]);
            foreach ($it as $value) {
                echo $value, "|";
            }
            $object = new ParentCallingObject(["x" => 8]);
            echo $object->x, "|", $object->getFlags(), "|";
            $heap = new ParentCallingHeap();
            $heap->insert(1);
            $heap->insert(2);
            echo $heap->count(), "|", $heap->extract();
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"rewind|7|8|2|2|2");
}

#[test]
fn spl_userland_countable_uses_internal_interface_metadata() {
    let result = execute_source(
        r#"<?php
            class Counted implements Countable {
                public function count(): int {
                    return 4;
                }
            }
            $value = new Counted();
            echo ($value instanceof Countable) ? "countable|" : "no|";
            echo count($value);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"countable|4");
}

#[test]
fn spl_array_object_supports_array_access_iteration_and_exchange() {
    let result = execute_source(
        r#"<?php
            $object = new ArrayObject(["a" => 1]);
            $object["b"] = 2;
            $object->append(3);
            echo $object["a"], "|", $object->offsetExists("b") ? "yes|" : "no|";
            foreach ($object as $key => $value) {
                echo $key, "=", $value, "|";
            }
            $old = $object->exchangeArray(["z" => 9]);
            echo count($object), "|", $old["a"], "|", $object["z"];
            $recursive = new ArrayObject([1, [2]], 0, "RecursiveArrayIterator");
            echo "|", $recursive->getIteratorClass(), "|";
            echo $recursive->getIterator() instanceof RecursiveArrayIterator ? "recursive|" : "plain|";
            try {
                new RecursiveIteratorIterator(new ArrayObject([1]));
            } catch (InvalidArgumentException $e) {
                echo $e->getMessage(), "|";
            }
            $payload = (new ArrayObject([1], 1))->__serialize();
            echo count($payload), "|", $payload[0], "|", $payload[3] === null ? "null" : "class";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"1|yes|a=1|b=2|0=3|1|1|9|RecursiveArrayIterator|recursive|An instance of RecursiveIterator or IteratorAggregate creating it is required|4|1|null"
        );
}

#[test]
fn spl_recursive_iterator_iterator_accepts_recursive_arrayobject_aggregate() {
    let result = execute_source(
        r#"<?php
            class Menu extends ArrayObject {
                function getIterator(): RecursiveArrayIterator {
                    echo "get|";
                    return new RecursiveArrayIterator($this->getArrayCopy());
                }
            }
            class MenuOutput extends RecursiveIteratorIterator {
                function __construct(Menu $it) {
                    parent::__construct($it);
                }
            }
            foreach (new MenuOutput(new Menu([1, [2]])) as $key => $value) {
                echo $key, ":", $value, ";";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"get|0:1;0:2;");
}

#[test]
fn spl_fixed_array_supports_bounds_checked_array_access() {
    let result = execute_source(
        r#"<?php
            $fixed = new SplFixedArray(3);
            $fixed[1] = "middle";
            echo $fixed->getSize(), "|", count($fixed), "|", $fixed[1], "|";
            foreach ($fixed as $key => $value) {
                echo $key, "=", $value, "|";
            }
            $fixed->setSize(2);
            echo $fixed->getSize();
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3|3|middle|0=|1=middle|2=|2");
}

#[test]
fn spl_fixed_array_var_dump_uses_numeric_debug_entries() {
    let result = execute_source(
        r#"<?php
            $fixed = new SplFixedArray(0);
            $value = 1;
            $array = [&$value];
            $fixed->__unserialize($array);
            var_dump($fixed);
            unset($fixed[0]);
            var_dump($fixed);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("object(SplFixedArray)#"), "{output}");
    assert!(output.contains("  [0]=>\n  int(1)\n"), "{output}");
    assert!(output.contains("  [0]=>\n  NULL\n"), "{output}");
    assert!(!output.contains("__entries"), "{output}");
}

#[test]
fn spl_fixed_array_object_vars_hide_internal_storage() {
    let result = execute_source(
        r#"<?php
            #[AllowDynamicProperties]
            class MySplFixedArray extends SplFixedArray {}
            $array = new MySplFixedArray(2);
            $array->{0} = [];
            var_dump(get_object_vars($array));
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"array(1) {\n  [0]=>\n  array(0) {\n  }\n}\n"
    );
}

#[test]
fn spl_object_storage_uses_runtime_object_identity() {
    let result = execute_source(
        r#"<?php
            class StdlibBox {}
            $a = new StdlibBox();
            $b = new StdlibBox();
            $storage = new SplObjectStorage();
            $storage->attach($a, "alpha");
            $storage->attach($b, "beta");
            echo $storage->contains($a) ? "has-a|" : "missing|";
            echo $storage->offsetGet($b), "|", count($storage), "|";
            foreach ($storage as $index => $object) {
                echo $index, ":", ($object instanceof StdlibBox) ? "box|" : "no|";
            }
            $storage->detach($a);
            echo count($storage);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"has-a|beta|2|0:box|1:box|1");
}

#[test]
fn spl_object_storage_array_access_accepts_object_keys() {
    let result = execute_source(
        r#"<?php
            $storage = new SplObjectStorage();
            $object = new stdClass();
            $storage[$object] = "some_value";
            echo $storage->offsetGet($object), "|", $storage[$object];
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"some_value|some_value");
}

#[test]
fn spl_object_storage_debug_info_returns_mutable_storage_records() {
    let result = execute_source(
        r#"<?php
            $storage = new SplObjectStorage();
            $object = new stdClass();
            $storage[$object] = 1;
            $debug = $storage->__debugInfo();
            $records = $debug[array_key_first($debug)];
            unset($debug);
            $records[0]["obj"] = new stdClass();
            echo count($records), "|";
            echo $records[0]["obj"] instanceof stdClass ? "object|" : "missing|";
            echo $records[0]["inf"];
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|object|1");
}

#[test]
fn spl_object_storage_var_dump_uses_private_storage_debug_view() {
    let result = execute_source(
        r#"<?php
            $storage = new SplObjectStorage();
            $object = new stdClass();
            $storage[$object] = 1;
            $storage->removeAll($storage);
            var_dump($storage);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("[\"storage\":\"SplObjectStorage\":private]=>\n  array(0)"),
        "{output}"
    );
    assert!(!output.contains("__storage"), "{output}");
    assert!(!output.contains("__position"), "{output}");
}

#[test]
fn spl_object_storage_serialize_snapshots_entries_during_magic_mutation() {
    let result = execute_source(
        r#"<?php
            class C {
                function __serialize(): array {
                    global $store;
                    $store->removeAll($store);
                    return [];
                }
            }
            $store = new SplObjectStorage();
            $store[new C()] = new stdClass();
            var_dump($store->serialize());
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("x:i:1;O:1:\"C\":0:{},O:8:\"stdClass\":0:{};m:a:0:{}"),
        "{output}"
    );
}

#[test]
fn spl_object_storage_setinfo_observes_info_destructor_mutation() {
    let result = execute_source(
        r#"<?php
            class C {
                function __destruct() {
                    global $store;
                    $store->removeAll($store);
                }
            }

            $o = new stdClass;
            $store = new SplObjectStorage;
            $store[$o] = new C;
            $store->setInfo(1);
            var_dump($store);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("[\"storage\":\"SplObjectStorage\":private]=>\n  array(0)"),
        "{output}"
    );
}

#[test]
fn spl_doubly_linked_list_serialize_observes_live_mutation() {
    let result = execute_source(
        r#"<?php
            class C {
                function __serialize(): array {
                    global $list;
                    $list->pop();
                    return [];
                }
            }
            $list = new SplDoublyLinkedList();
            $list->add(0, new C());
            $list->add(1, 1);
            var_dump($list->serialize());
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("i:0;:O:1:\"C\":0:{}"), "{output}");
}

#[test]
fn spl_stack_queue_and_doubly_linked_list_mvp_use_simple_storage() {
    let result = execute_source(
        r#"<?php
            $stack = new SplStack();
            $stack->push("a");
            $stack->push("b");
            echo $stack->top(), "|", $stack->pop(), "|", $stack->count(), "|";
            $queue = new SplQueue();
            $queue->push("x");
            $queue->push("y");
            echo $queue->shift(), "|", $queue->bottom(), "|";
            $list = new SplDoublyLinkedList();
            $list->push(4);
            $list->unshift(3);
            foreach ($list as $key => $value) {
                echo $key, "=", $value, "|";
            }
            echo ($stack instanceof SplDoublyLinkedList) ? "list" : "no";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"b|b|1|x|y|0=3|1=4|list");
}

#[test]
fn spl_file_info_and_file_object_use_allowed_local_files() {
    let root = std::env::temp_dir().join(format!("phrust-spl-file-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let file = root.join("items.csv");
    std::fs::write(&file, "name,qty\napple,2\n").expect("fixture should be written");
    let path = file.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $info = new SplFileInfo("{path}");
            echo $info->getFilename(), "|", $info->getBasename(".csv"), "|";
            echo ($info->getSize() > 0) ? "size|" : "empty|";
            echo ($info->getRealPath() !== false) ? "real|" : "missing|";
            $file = new SplFileObject("{path}");
            echo $file->fgets();
            $file->rewind();
            echo (string) $file;
            foreach ($file as $line => $text) {{
                echo $line, ":", $text;
            }}
            $file->rewind();
            $row = $file->fgetcsv();
            echo "|", $row[0], ":", $row[1], "|";
            echo ($file instanceof SplFileInfo) ? "info|" : "no|";
            $temp = new SplTempFileObject();
            echo (string) $temp, "temp";
            try {{
                $temp->ftruncate(-1);
            }} catch (ValueError $e) {{
                echo "|", $e->getMessage();
            }}
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "items.csv|items|size|real|name,qty\nname,qty\n0:name,qty\n1:apple,2\n|name:qty|info|temp|SplFileObject::ftruncate(): Argument #1 ($size) must be greater than or equal to 0"
    );
}

#[test]
fn spl_file_object_unconstructed_subclass_method_reports_invalid_state() {
    let result = execute_source(
        r#"<?php
            class bug8318 extends SplFileObject {
                public function __construct() {
                }

                public function fpassthru(): int {
                    return 0;
                }
            }

            $file = new bug8318;
            try {
                $file->fpassthru();
            } catch (Error $e) {
                echo get_class($e), "|", $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "Error|The parent constructor was not called: the object is in an invalid state"
    );
}

#[test]
fn spl_file_object_unconstructed_subclass_error_uses_error_private_labels() {
    let result = execute_source(
        r#"<?php
            class bug8318 extends SplFileObject {
                public function __construct() {
                }

                public function fpassthru(): int {
                    return 0;
                }
            }

            $file = new bug8318;
            try {
                $file->fpassthru();
            } catch (Error $e) {
                var_dump($e);
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("[\"string\":\"Error\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"trace\":\"Error\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"previous\":\"Error\":private]=>"),
        "{output}"
    );
    assert!(
        !output.contains("[\"string\":\"Exception\":private]=>"),
        "{output}"
    );
}

#[test]
fn spl_recursive_directory_iterator_walks_allowed_local_files() {
    let root = std::env::temp_dir().join(format!(
        "phrust-spl-recursive-directory-{}",
        std::process::id()
    ));
    let nested = root.join("sub");
    std::fs::create_dir_all(&nested).expect("temp nested directory should be created");
    let top_file = root.join("top.txt");
    let nested_file = nested.join("nested.txt");
    std::fs::write(&top_file, "top").expect("top fixture should be written");
    std::fs::write(&nested_file, "nested").expect("nested fixture should be written");
    let root_path = root.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $it = new RecursiveIteratorIterator(
                new RecursiveDirectoryIterator("{root_path}", FilesystemIterator::SKIP_DOTS | FilesystemIterator::UNIX_PATHS)
            );
            $items = [];
            foreach ($it as $key => $info) {{
                $items[] = basename($key) . ":" . get_class($info) . ":" . $info->getFilename() . ":" . ($info->isFile() ? "file" : "other");
            }}
            sort($items);
            echo implode("|", $items), "|";
            $fs = new FilesystemIterator("{root_path}", FilesystemIterator::KEY_AS_FILENAME | FilesystemIterator::CURRENT_AS_PATHNAME | FilesystemIterator::SKIP_DOTS);
            foreach ($fs as $key => $path) {{
                if ($key === "top.txt") {{
                    echo "fs:", $key, ":", basename($path), ":", $fs->getFlags();
                }}
            }}
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                top_file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "nested.txt:SplFileInfo:nested.txt:file|top.txt:SplFileInfo:top.txt:file|fs:top.txt:top.txt:4384"
    );
    let _ = std::fs::remove_file(nested_file);
    let _ = std::fs::remove_file(top_file);
    let _ = std::fs::remove_dir(nested);
    let _ = std::fs::remove_dir(root);
}

#[test]
fn spl_parent_iterator_filters_recursive_parents_for_rii_modes() {
    let result = execute_source(
        r#"<?php
            $it = new ParentIterator(new RecursiveArrayIterator([1, [21, 22, [231]], 3]));
            $leaves = [];
            foreach (new RecursiveIteratorIterator($it) as $key => $value) {
                $leaves[] = $key . ":" . gettype($value);
            }
            echo count($leaves), "|";
            foreach (new RecursiveIteratorIterator($it, RecursiveIteratorIterator::SELF_FIRST) as $key => $value) {
                echo $key, ":", count($value), ";";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0|1:3;2:1;");
}

#[test]
fn spl_recursive_iterator_iterator_max_depth_filters_and_resets() {
    let result = execute_source(
        r#"<?php
            $it = new RecursiveIteratorIterator(new RecursiveArrayIterator([1, [21, [331]], 4]));
            var_dump($it->getMaxDepth());
            $it->setMaxDepth(1);
            var_dump($it->getMaxDepth());
            foreach ($it as $value) {
                echo $it->getDepth(), ":", $value, "|";
            }
            $it->setMaxDepth();
            var_dump($it->getMaxDepth());
            try {
                $it->setMaxDepth(-2);
            } catch (ValueError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(false)\nint(1)\n0:1|1:21|0:4|bool(false)\nRecursiveIteratorIterator::setMaxDepth(): Argument #1 ($maxDepth) must be greater than or equal to -1"
    );
}

#[test]
fn zip_archive_create_overwrite_writes_local_entries() {
    let root = std::env::temp_dir().join(format!("phrust-zip-write-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let source_file = root.join("source.txt");
    let archive_file = root.join("export.zip");
    std::fs::write(&source_file, "payload").expect("fixture should be written");
    let source_path = source_file.to_string_lossy().replace('\\', "\\\\");
    let archive_path = archive_file.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            echo ZipArchive::CREATE, ":", ZipArchive::OVERWRITE, ":", ZipArchive::FL_OVERWRITE, ":", ZipArchive::LENGTH_TO_END, "|";
            $zip = new ZipArchive();
            echo ($zip->open("{archive_path}", ZipArchive::CREATE | ZipArchive::OVERWRITE) === true) ? "open|" : "bad-open|";
            echo $zip->addEmptyDir("templates") ? "dir|" : "bad-dir|";
            echo $zip->addFile("{source_path}", "theme/source.txt") ? "file|" : "bad-file|";
            echo $zip->addFromString("templates/index.html", "alpha") ? "string|" : "bad-string|";
            echo $zip->addFromString("templates/index.html", "beta") ? "replace|" : "bad-replace|";
            echo $zip->close() ? "close|" : "bad-close|";
            $read = new ZipArchive();
            echo $read->open("{archive_path}") ? "read|" : "bad-read|";
            echo ($read instanceof Countable) ? "countable|" : "not-countable|";
            echo $read->count(), ":", count($read), "|", $read->getFromName("templates/index.html"), "|", $read->getFromName("theme/source.txt");
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                source_file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "1:8:8192:0|open|dir|file|string|replace|close|read|countable|3:3|beta|payload"
    );
    let _ = std::fs::remove_file(archive_file);
    let _ = std::fs::remove_file(source_file);
    let _ = std::fs::remove_dir(root);
}

#[test]
fn spl_regex_iterator_filters_recursive_directory_paths_with_get_match() {
    let root = std::env::temp_dir().join(format!(
        "phrust-spl-regex-recursive-directory-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temp directory should be created");
    let index_file = root.join("index.html");
    let upper_file = root.join("about.HTML");
    let skip_file = root.join("style.css");
    std::fs::write(&index_file, "index").expect("index fixture should be written");
    std::fs::write(&upper_file, "about").expect("upper fixture should be written");
    std::fs::write(&skip_file, "css").expect("skip fixture should be written");
    let root_path = root.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $it = new RecursiveIteratorIterator(
                new RecursiveDirectoryIterator("{root_path}", FilesystemIterator::SKIP_DOTS | FilesystemIterator::UNIX_PATHS)
            );
            $regex = new RegexIterator($it, "/^.+\.html$/i", RecursiveRegexIterator::GET_MATCH);
            $items = [];
            foreach ($regex as $path => $file) {{
                $items[] = basename($path) . ":" . gettype($file) . ":" . basename($file[0]);
            }}
            sort($items);
            echo implode("|", $items);
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                index_file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "about.HTML:array:about.HTML|index.html:array:index.html"
    );
    let _ = std::fs::remove_file(skip_file);
    let _ = std::fs::remove_file(upper_file);
    let _ = std::fs::remove_file(index_file);
    let _ = std::fs::remove_dir(root);
}

#[cfg(unix)]
#[test]
fn spl_file_info_reports_link_target_created_by_symlink() {
    let root = std::env::temp_dir().join(format!("phrust-spl-link-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let file = root.join("target.txt");
    let link = root.join("link.txt");
    std::fs::write(&file, "payload").expect("fixture should be written");
    let file_path = file.to_string_lossy().replace('\\', "\\\\");
    let link_path = link.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $target = "{file_path}";
            $link = "{link_path}";
            echo symlink($target, $link) ? "created|" : "failed|";
            $info = new SplFileInfo($link);
            echo $info->isLink() ? "link|" : "not-link|";
            echo $info->getLinkTarget() === $target ? "target" : "different";
            unlink($link);
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"created|link|target");
    let _ = std::fs::remove_file(link);
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_dir(root);
}

#[test]
fn spl_internal_file_subclass_uses_parent_storage_and_methods() {
    let root =
        std::env::temp_dir().join(format!("phrust-spl-file-subclass-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let file = root.join("child.txt");
    std::fs::write(&file, "payload").expect("fixture should be written");
    let path = file.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            class MyFileInfo extends SplFileInfo {{}}
            $info = new MyFileInfo("{path}");
            echo ($info instanceof MyFileInfo) ? "self|" : "no|";
            echo ($info instanceof SplFileInfo) ? "info|" : "no|";
            echo $info->getFilename(), "|", $info->getExtension();
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"self|info|child.txt|txt");
}

#[test]
fn spl_file_info_get_path_info_honors_subclass_argument() {
    let result = execute_source(
        r#"<?php
            $file = new SplTempFileObject();
            class SplFileInfoChild extends SplFileInfo {}
            class BadSplFileInfo {}
            $info = $file->getPathInfo("SplFileInfoChild");
            echo get_class($info), "|";
            echo ($info instanceof SplFileInfoChild) ? "child|" : "no|";
            echo ($info instanceof SplFileInfo) ? "info|" : "no|";
            var_dump($info);
            try {
                $file->getPathInfo("BadSplFileInfo");
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.starts_with("SplFileInfoChild|child|info|"),
        "{output}"
    );
    assert!(output.contains("object(SplFileInfoChild)#"), "{output}");
    assert!(
        output.contains("[\"pathName\":\"SplFileInfo\":private]=>\n  string(4) \"php:\""),
        "{output}"
    );
    assert!(
        output.contains("[\"fileName\":\"SplFileInfo\":private]=>\n  string(4) \"php:\""),
        "{output}"
    );
    assert!(!output.contains("__path"), "{output}");
    assert!(output.contains("SplFileInfo::getPathInfo(): Argument #1 ($class) must be a class name derived from SplFileInfo or null, BadSplFileInfo given"), "{output}");
}

#[test]
fn spl_file_object_rejects_repeated_constructor_call() {
    let root = std::env::temp_dir().join(format!("phrust-spl-file-repeat-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root should be created");
    let file = root.join("repeat.txt");
    std::fs::write(&file, "payload").expect("fixture should be written");
    let path = file.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"<?php
            $file = new SplFileObject("{path}");
            try {{
                $file->__construct("{path}");
            }} catch (Error $e) {{
                echo $e->getMessage();
            }}
            "#
    );
    let result = execute_source_with_options(
        &source,
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli(
                file.to_string_lossy().into_owned(),
                Vec::new(),
            )
            .with_filesystem_capabilities(
                php_runtime::api::FilesystemCapabilities::none()
                    .with_allowed_roots(vec![root.clone()]),
            ),
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"Cannot call constructor twice");
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_dir(root);
}

#[test]
fn spl_heap_runtime_classes_order_and_count_entries() {
    let result = execute_source(
        r#"<?php
            $max = new SplMaxHeap();
            $max->insert(1);
            $max->insert(3);
            $max->insert(2);
            echo $max instanceof SplHeap ? "heap|" : "no|";
            echo count($max), "|", $max->top(), "|", $max->extract(), "|", $max->extract(), "|";
            $min = new SplMinHeap();
            $min->insert(2);
            $min->insert(1);
            echo $min->extract(), "|", $max->isEmpty() ? "empty|" : "items|";
            foreach ($max as $key => $value) {
                echo $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"heap|3|3|3|2|1|items|0=1|");
}

#[test]
fn spl_heap_empty_extract_raises_runtime_exception() {
    let result = execute_source(
        r#"<?php
            try {
                (new SplMaxHeap())->extract();
            } catch (RuntimeException $e) {
                echo $e->getMessage();
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"Can't extract from an empty heap"
    );
}

#[test]
fn spl_heap_user_subclass_uses_parent_storage_and_iteration() {
    let result = execute_source(
        r#"<?php
            class MyHeap extends SplHeap {
                public function compare($a, $b): int {
                    return $a <=> $b;
                }
            }
            $heap = new MyHeap();
            $heap->insert(1);
            $heap->insert(3);
            $heap->insert(2);
            echo ($heap instanceof MyHeap) ? "self|" : "no|";
            echo ($heap instanceof SplHeap) ? "heap|" : "no|";
            foreach ($heap as $value) {
                echo $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"self|heap|3|2|1|");
}

#[test]
fn spl_heap_serialize_returns_user_properties_and_internal_state() {
    let result = execute_source(
        r#"<?php
            class CustomHeap extends SplMaxHeap {
                public $field = 0;
            }
            class CustomPriorityQueue extends SplPriorityQueue {
                public $field = 0;
            }
            $heap = (new CustomHeap())->__serialize();
            echo $heap[0]["field"], "|", $heap[1]["flags"], "|", count($heap[1]["heap_elements"]), "|";
            $queue = (new CustomPriorityQueue())->__serialize();
            echo $queue[0]["field"], "|", $queue[1]["flags"], "|", count($queue[1]["heap_elements"]);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0|0|0|0|1|0");
}

#[test]
fn spl_doubly_linked_list_var_dump_uses_private_debug_view() {
    let result = execute_source(
        r#"<?php
            $stack = new SplStack();
            $stack[] = new stdClass();
            unset($stack[0]);
            var_dump($stack);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert!(
        output.contains("[\"flags\":\"SplDoublyLinkedList\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"dllist\":\"SplDoublyLinkedList\":private]=>"),
        "{output}"
    );
    assert!(output.contains("int(6)"), "{output}");
    assert!(output.contains("array(0)"), "{output}");
    assert!(!output.contains("__entries"), "{output}");
}

#[test]
fn spl_heap_var_dump_uses_private_debug_view() {
    let result = execute_source(
        r#"<?php
            $heap = new SplMaxHeap();
            $heap->insert(1);
            var_dump($heap);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert!(
        output.contains("[\"flags\":\"SplHeap\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"isCorrupted\":\"SplHeap\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"heap\":\"SplHeap\":private]=>"),
        "{output}"
    );
    assert!(!output.contains("__entries"), "{output}");
}

#[test]
fn spl_heap_var_dump_marks_self_reference_recursion() {
    let result = execute_source(
        r#"<?php
            $heap = new SplMaxHeap();
            $heap->insert($heap);
            var_dump($heap);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert!(
        output.contains("[\"heap\":\"SplHeap\":private]=>"),
        "{output}"
    );
    assert!(output.contains("*RECURSION*"), "{output}");
}

#[test]
fn spl_heap_iteration_compare_exception_marks_corruption() {
    let result = execute_source(
        r#"<?php
            class ExtHeap extends SplMaxHeap {
                public $fail = false;
                public function compare($left, $right): int {
                    if ($this->fail) {
                        throw new Exception('Corrupting heap', 99);
                    }
                    return 0;
                }
            }
            $heap = new ExtHeap();
            $heap->insert(array('foobar'));
            $heap->insert(array('foobar1'));
            $heap->insert(array('foobar2'));
            try {
                $heap->fail = true;
                foreach ($heap as $value) {}
            } catch (Exception $exception) {
                echo $exception->getCode(), '|';
            }
            var_dump($heap);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert!(output.starts_with("99|object(ExtHeap)#"), "{output}");
    assert!(output.contains("[\"fail\"]=>"), "{output}");
    assert!(
        output.contains("[\"isCorrupted\":\"SplHeap\":private]=>\n  bool(true)"),
        "{output}"
    );
    assert!(output.contains("string(7) \"foobar2\""), "{output}");
    assert!(!output.contains("string(6) \"foobar\""), "{output}");
}

#[test]
fn spl_priority_queue_var_dump_uses_private_debug_view() {
    let result = execute_source(
        r#"<?php
            $queue = new SplPriorityQueue();
            $queue->insert("a", 1);
            var_dump($queue);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = String::from_utf8_lossy(result.output.as_bytes());
    assert!(
        output.contains("[\"flags\":\"SplPriorityQueue\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"isCorrupted\":\"SplPriorityQueue\":private]=>"),
        "{output}"
    );
    assert!(
        output.contains("[\"heap\":\"SplPriorityQueue\":private]=>"),
        "{output}"
    );
    assert!(!output.contains("__entries"), "{output}");
}

#[test]
fn spl_internal_subclass_user_methods_fall_through_after_marker_dispatch() {
    let result = execute_source(
        r#"<?php
            class MyRegexIterator extends RegexIterator {
                public function show() {
                    return $this->getRegex();
                }
            }
            $it = new MyRegexIterator(new ArrayIterator(["cat", "dog"]), "/cat/");
            echo $it->show(), "|";
            foreach ($it as $value) {
                echo $value;
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"/cat/|cat");
}

#[test]
fn spl_append_iterator_preserves_attached_iterators_and_indices() {
    let result = execute_source(
        r#"<?php
            $it = new AppendIterator();
            $first = new ArrayIterator([1]);
            $second = new ArrayIterator([21, 22]);
            $it->append($first);
            $it->append($second);
            $attached = $it->getArrayIterator()->getArrayCopy();
            echo count($attached), "|";
            echo ($attached[0] === $first && $attached[1] === $second) ? "same|" : "diff|";
            foreach ($it as $key => $value) {
                echo $it->getIteratorIndex(), ":", $key, ":", $value, "|";
            }
            ob_start();
            var_dump($it->getArrayIterator());
            $dump = ob_get_clean();
            echo str_contains($dump, '["storage":"ArrayIterator":private]') ? "storage" : "missing";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"2|same|0:0:1|1:0:21|1:1:22|storage"
    );
}

#[test]
fn spl_append_iterator_parent_constructor_state_is_enforced() {
    let result = execute_source(
        r#"<?php
            class MyAppendIterator extends AppendIterator {
                public function __construct() {}
                public function parentConstruct() { parent::__construct(); }
            }
            $it = new MyAppendIterator();
            try {
                $it->append(new ArrayIterator([1]));
            } catch (Error $e) {
                echo $e->getMessage(), "|";
            }
            $it->parentConstruct();
            try {
                $it->parentConstruct();
            } catch (BadMethodCallException $e) {
                echo $e->getMessage(), "|";
            }
            $it->append(new ArrayIterator([2]));
            foreach ($it as $value) {
                echo $value;
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"The object is in an invalid state as the parent constructor was not called|AppendIterator::getIterator() must be called exactly once per instance|2"
        );
}

#[test]
fn spl_append_iterator_parent_append_rewinds_attached_iterator_once() {
    let result = execute_source(
        r#"<?php
            class MyArrayIterator extends ArrayIterator {
                public function rewind(): void {
                    echo "inner-rewind|";
                    parent::rewind();
                }
            }
            class MyAppendIterator extends AppendIterator {
                public function __construct() {}
                public function append(Iterator $iterator): void {
                    echo "append|";
                    parent::append($iterator);
                }
                public function parentConstruct(): void {
                    parent::__construct();
                }
            }
            $inner = new MyArrayIterator([1, 2]);
            foreach ($inner as $_) {}
            $append = new MyAppendIterator();
            $append->parentConstruct();
            $append->append($inner);
            $append->append($inner);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"inner-rewind|append|inner-rewind|append|"
    );
}

#[test]
fn spl_multiple_iterator_callbacks_survive_self_detach() {
    let result = execute_source(
        r#"<?php
            class DetachOnRewind implements Iterator {
                public function __construct(private MultipleIterator $parent) {}
                public function rewind(): void {
                    $this->parent->detachIterator($this);
                    echo "rewind|";
                }
                public function next(): void {}
                public function current(): mixed { return 0; }
                public function key(): mixed { return 0; }
                public function valid(): bool { return false; }
            }
            class DetachOnCurrent implements Iterator {
                public function __construct(private MultipleIterator $parent) {}
                public function rewind(): void {}
                public function next(): void {}
                public function current(): mixed {
                    $this->parent->detachIterator($this);
                    return "C";
                }
                public function key(): mixed { return "k"; }
                public function valid(): bool { return true; }
            }
            $it = new MultipleIterator();
            $it->attachIterator(new DetachOnRewind($it));
            $it->rewind();
            echo $it->countIterators(), "|";
            $it = new MultipleIterator(MultipleIterator::MIT_NEED_ALL | MultipleIterator::MIT_KEYS_ASSOC);
            $it->attachIterator(new DetachOnCurrent($it), "name");
            $current = $it->current();
            echo $current["name"], "|", $it->countIterators();
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"rewind|0|C|0");
}

#[test]
fn redis_endpoint_client_fails_closed_without_daemon() {
    let result = execute_source(
        r#"<?php
            $redis = new Redis();
            echo class_exists("Redis", false) ? "class|" : "missing|";
            echo $redis instanceof Redis ? "instance|" : "not-instance|";
            echo method_exists($redis, "getMultiple") ? "method|" : "no-method|";
            echo $redis->connect("127.0.0.1", 1, 0.001) ? "connected|" : "offline|";
            echo $redis->isConnected() ? "still-on|" : "closed|";
            echo $redis->set("a", "1") ? "set|" : "no-set|";
            echo $redis->get("a") === false ? "miss|" : "fake-hit|";
            echo $redis->mset(["b" => "2"]) ? "mset|" : "no-mset|";
            echo $redis->mget(["a", "b"]) === false ? "no-mget|" : "fake-mget|";
            echo $redis->hSet("h", "f", "v") === false ? "no-hset|" : "fake-hset|";
            echo $redis->lPush("l", "x") === false ? "no-lpush|" : "fake-lpush|";
            echo $redis->lRange("l", 0, -1) === false ? "no-lrange|" : "fake-lrange|";
            echo $redis->ttl("a") === false ? "no-ttl" : "fake-ttl";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"class|instance|method|offline|closed|no-set|miss|no-mset|no-mget|no-hset|no-lpush|no-lrange|no-ttl"
    );
}

#[test]
fn memcached_endpoint_client_fails_closed_without_daemon() {
    let result = execute_source(
        r#"<?php
            $memcached = new Memcached();
            echo class_exists("Memcached", false) ? "class|" : "missing|";
            echo $memcached instanceof Memcached ? "instance|" : "not-instance|";
            echo method_exists($memcached, "getMulti") ? "method|" : "no-method|";
            echo Memcached::RES_SUCCESS, ":", Memcached::RES_NOTFOUND, ":", Memcached::RES_FAILURE, "|";
            echo $memcached->addServer("127.0.0.1", 1) ? "server|" : "no-server|";
            echo $memcached->getResultCode(), ":", $memcached->getResultMessage(), "|";
            echo $memcached->set("a", "1") ? "set|" : "no-set|";
            echo $memcached->get("a") === false ? "miss|" : "fake-hit|";
            echo $memcached->setMulti(["b" => "2"]) ? "mset|" : "no-mset|";
            echo $memcached->getMulti(["a", "b"]) === false ? "no-mget|" : "fake-mget|";
            echo $memcached->increment("n", 2, 10) === false ? "no-incr|" : "fake-incr|";
            echo $memcached->delete("a") ? "deleted" : "not-deleted";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"class|instance|method|0:16:1|no-server|1:FAILURE|no-set|miss|no-mset|no-mget|no-incr|not-deleted"
    );
}

#[test]
fn imagick_surface_fails_closed_without_imagemagick_backend() {
    let result = execute_source(
        r#"<?php
            echo extension_loaded("imagick") ? "loaded|" : "missing|";
            echo class_exists("Imagick", false) ? "class|" : "no-class|";
            echo class_exists("ImagickDraw", false) ? "draw|" : "no-draw|";
            echo method_exists("Imagick", "readImage") ? "method|" : "no-method|";
            echo method_exists("Imagick", "readImageBlob") ? "blob|" : "no-blob|";
            echo method_exists("Imagick", "getImageWidth") ? "width|" : "no-width|";
            echo method_exists("Imagick", "stripImage") ? "strip|" : "no-strip|";
            $reflection = new ReflectionClass("Imagick");
            echo $reflection->getName(), ":", $reflection->getExtensionName(), "|";
            new Imagick();
            "#,
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.output.as_bytes(),
        b"loaded|class|draw|method|blob|width|strip|Imagick:imagick|"
    );
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNSUPPORTED_IMAGICK");
}

#[test]
fn xsl_surface_fails_closed_without_libxslt_backend() {
    let result = execute_source(
        r#"<?php
            echo extension_loaded("xsl") ? "loaded|" : "missing|";
            echo class_exists("XSLTProcessor", false) ? "class|" : "no-class|";
            echo method_exists("XSLTProcessor", "hasExsltSupport") ? "method|" : "no-method|";
            $reflection = new ReflectionClass("XSLTProcessor");
            echo $reflection->getName(), ":", $reflection->getExtensionName(), "|";
            new XSLTProcessor();
            "#,
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.output.as_bytes(),
        b"loaded|class|method|XSLTProcessor:xsl|"
    );
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_UNSUPPORTED_XSL");
}

#[test]
fn spl_priority_queue_extract_flags_select_output_shape() {
    let result = execute_source(
        r#"<?php
            $pq = new SplPriorityQueue();
            echo $pq->getExtractFlags(), "|";
            $pq->insert("a", 1);
            $pq->insert("b", 2);
            echo $pq->top(), "|";
            $pq->setExtractFlags(SplPriorityQueue::EXTR_PRIORITY);
            echo $pq->top(), "|";
            $pq->setExtractFlags(SplPriorityQueue::EXTR_BOTH);
            $both = $pq->top();
            echo $both["data"], ":", $both["priority"], "|";
            foreach ($pq as $key => $value) {
                echo $key, "=", $value["data"], ";";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|b|2|b:2|1=b;0=a;");
}

#[test]
fn spl_iterator_flag_constants_include_inherited_internal_parents() {
    let result = execute_source(
        r#"<?php
            class MyRegexIterator extends RegexIterator {}
            echo MyRegexIterator::USE_KEY, "|";
            echo CachingIterator::FULL_CACHE, "|", CachingIterator::CALL_TOSTRING, "|";
            echo RecursiveIteratorIterator::LEAVES_ONLY, "|", RecursiveIteratorIterator::CHILD_FIRST, "|";
            echo RecursiveTreeIterator::BYPASS_CURRENT, "|", RecursiveTreeIterator::PREFIX_RIGHT, "|";
            echo MultipleIterator::MIT_NEED_ALL, "|", MultipleIterator::MIT_KEYS_ASSOC;
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|256|1|0|2|4|5|1|2");
}

#[test]
fn spl_caching_iterator_supports_array_access_offsets() {
    let result = execute_source(
        r#"<?php
            $it = new CachingIterator(new ArrayIterator([1, 2]), CachingIterator::FULL_CACHE);
            foreach ($it as $value) {
                echo $value, ":", $it->count(), "|";
            }
            echo isset($it[0]) ? "set|" : "missing|";
            echo $it[0], "|";
            $it[2] = "x";
            $it["name"] = "y";
            echo $it[2], "|", $it["name"], "|";
            unset($it[0]);
            echo isset($it[0]) ? "set" : "missing";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1:1|2:2|set|1|x|y|missing");
}

#[test]
fn spl_caching_iterator_full_cache_offsets_follow_cached_entries() {
    let result = execute_source(
        r#"<?php
            $items = [1, 2, [31, 32, [331]], 4];
            $it = new CachingIterator(
                new RecursiveIteratorIterator(new RecursiveArrayIterator($items)),
                CachingIterator::FULL_CACHE
            );
            foreach ($it as $key => $value) {
                echo $key, "=>", $value, "|";
            }
            echo $it[0], ":", $it[1], ":", $it[3], "|";
            $it[2] = "foo";
            $it[3] = "bar";
            $it["baz"] = "25";
            echo $it[2], ":", $it[3], ":", $it["baz"], "|";
            unset($it[0], $it[2], $it["baz"]);
            echo isset($it[0]) ? "set" : "missing";
            echo ":", isset($it[1]) ? "set" : "missing";
            echo ":", isset($it[2]) ? "set" : "missing";
            echo ":", isset($it[3]) ? "set" : "missing";
            echo ":", isset($it["baz"]) ? "set" : "missing", "|";
            $it->rewind();
            echo isset($it[0]) ? "prefetched" : "missing";
            echo ":", isset($it[1]) ? "set" : "missing";
            echo ":", isset($it[2]) ? "set" : "missing";
            echo ":", isset($it[3]) ? "set" : "missing";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"0=>1|1=>2|0=>31|1=>32|0=>331|3=>4|331:32:4|foo:bar:25|missing:set:missing:set:missing|prefetched:missing:missing:missing"
        );
}

#[test]
fn spl_recursive_caching_iterator_reports_active_inner_has_next() {
    let result = execute_source(
        r#"<?php
            $items = [1, 2, [31, 32, [331]], 4];
            $it = new RecursiveIteratorIterator(
                new RecursiveCachingIterator(new RecursiveArrayIterator($items))
            );
            foreach ($it as $key => $value) {
                echo $key, "=>", $value, "\n";
                echo "hasNext: ", $it->getInnerIterator()->hasNext() ? "yes" : "no", "\n";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Warning: Array to string conversion"),
        "{output}"
    );
    assert!(
        output.contains("0=>331\nhasNext: no\n3=>4\nhasNext: no"),
        "{output}"
    );
    assert_eq!(
        output
            .matches("Warning: Array to string conversion")
            .count(),
        2
    );
}

#[test]
fn spl_multiple_iterator_tracks_attached_iterator_identity() {
    let result = execute_source(
        r#"<?php
            $one = new ArrayIterator([1]);
            $two = new ArrayIterator([2]);
            $multi = new MultipleIterator();
            $multi->attachIterator($one);
            $multi->attachIterator($two);
            echo $multi->countIterators(), "|";
            echo $multi->containsIterator($two) ? "yes|" : "no|";
            $multi->detachIterator($two);
            echo $multi->countIterators(), "|";
            echo $multi->containsIterator($two) ? "yes" : "no";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2|yes|1|no");
}

#[test]
fn spl_multiple_iterator_offset_set_requires_iterator_objects() {
    let result = execute_source(
        r#"<?php
            class MyIterator implements Iterator {
                public function valid(): bool { return false; }
                public function current(): mixed { return null; }
                public function key(): string { return ""; }
                public function next(): void {}
                public function rewind(): void {}
            }
            class MyAggregate implements IteratorAggregate {
                public function getIterator(): Traversable { throw new Error; }
            }
            $multi = new MultipleIterator();
            try {
                $multi[new stdClass()] = 1;
            } catch (TypeError $e) {
                echo $e->getMessage(), "|";
            }
            try {
                $multi[new MyAggregate()] = 1;
            } catch (TypeError $e) {
                echo $e->getMessage(), "|";
            }
            $iterator = new MyIterator();
            $multi[$iterator] = 1;
            echo $multi->countIterators(), "|";
            echo $multi->containsIterator($iterator) ? "yes" : "no";
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"Can only attach objects that implement the Iterator interface|Can only attach objects that implement the Iterator interface|1|yes"
        );
}

#[test]
fn spl_recursive_regex_iterator_descends_into_child_arrays() {
    let result = execute_source(
        r#"<?php
            class MyRecursiveRegexIterator extends RecursiveRegexIterator {
                function show() {
                    foreach (new RecursiveIteratorIterator($this) as $key => $value) {
                        echo $key, ":", $value, "|";
                    }
                }
            }
            $items = new RecursiveArrayIterator(["Foo", ["Bar"], "FooBar", ["Baz"], "Biz"]);
            $iterator = new MyRecursiveRegexIterator($items, "/Bar/");
            $iterator->show();
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0:Bar|2:FooBar|");
}

#[test]
fn spl_regex_iterator_digit_captures_shape_match_arrays() {
    let result = execute_source(
        r#"<?php
            $it = new RegexIterator(
                new ArrayIterator(["1", "1,2", "1,2,3"]),
                '/(\d),(\d)/',
                RegexIterator::GET_MATCH
            );
            foreach ($it as $key => $value) {
                echo $key, ":", $value[0], ":", $value[1], ":", $value[2], "|";
            }
            $it = new RegexIterator(
                new ArrayIterator(["1", "1,2"]),
                '/(\d)/',
                RegexIterator::ALL_MATCHES
            );
            foreach ($it as $key => $value) {
                echo $key, "=", count($value[0]), ":", count($value[1]), "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"1:1,2:1:2|2:1,2:1:2|0=1:1|1=2:2|"
    );
}

#[test]
fn spl_regex_iterator_split_shapes_values() {
    let result = execute_source(
        r#"<?php
            $it = new RegexIterator(
                new ArrayIterator(["1", "1,2", "1,2,3", ",", ",,"]),
                '/,/',
                RegexIterator::SPLIT
            );
            foreach ($it as $key => $value) {
                echo $key, ":", count($value), ":", $value[0], ":", $value[count($value) - 1], "|";
            }
            $it = new RegexIterator(
                new ArrayIterator(["1" => 0, "1,2" => 1, "1,2,3" => 2]),
                '/(\d),(\d)/',
                RegexIterator::SPLIT,
                RegexIterator::USE_KEY
            );
            foreach ($it as $key => $value) {
                echo $key, "=", count($value), ":", $value[0], ":", $value[count($value) - 1], "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"1:2:1:2|2:3:1:3|3:2::|4:3::|1,2=2::|1,2,3=2::,3|"
    );
}

#[test]
fn spl_regex_iterator_foreach_invokes_userland_accept() {
    let result = execute_source(
        r#"<?php
            class MyRegexIterator extends RegexIterator {
                public function accept(): bool {
                    echo "accept:", $this->key(), "|";
                    return parent::accept();
                }
            }
            $it = new MyRegexIterator(new ArrayIterator(["1", "1,2"]), '/(\d),(\d)/');
            foreach ($it as $key => $value) {
                echo $key, "=", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"accept:0|accept:1|1=1,2|");
}

#[test]
fn spl_regex_iterator_userland_accept_rejected_arrays_stay_raw() {
    let result = execute_source(
        r#"<?php
            class MyRegexIterator extends RegexIterator {
                public function accept(): bool {
                    @preg_match_all($this->getRegex(), (string) $this->current(), $sub);
                    $accepted = parent::accept();
                    var_dump($sub == $this->current());
                    return $accepted;
                }
            }
            $it = new MyRegexIterator(new ArrayIterator(["1,2", []]), '/(\d),(\d)/', RegexIterator::ALL_MATCHES);
            foreach ($it as $key => $value) {
                echo $key, '|';
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"bool(true)\n0|bool(false)\n");
}

#[test]
fn spl_recursive_iterator_iterator_reports_flattened_entry_depths() {
    let result = execute_source(
        r#"<?php
            class MyRecursiveFilterIterator extends RecursiveFilterIterator {
                function accept(): bool {
                    return true;
                }
            }
            $it = new RecursiveIteratorIterator(
                new MyRecursiveFilterIterator(
                    new RecursiveArrayIterator([1, [21, 22], 3])
                )
            );
            foreach ($it as $key => $value) {
                echo $it->getDepth(), ":", $key, ":", $value, "|";
            }
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0:0:1|1:0:21|1:1:22|0:2:3|");
}

#[test]
fn spl_recursive_iterator_iterator_direct_current_and_call_get_children() {
    let result = execute_source(
        r#"<?php
            $it = new RecursiveIteratorIterator(
                new RecursiveArrayIterator([[7, 8, 9], 1, 2, 3, [4, 5, 6]])
            );
            var_dump($it->current());
            $it->next();
            var_dump($it->current());
            try {
                $child = $it->callGetChildren();
            } catch (TypeError $e) {
                echo $e->getMessage(), "\n";
                $child = null;
            }
            var_dump($child);
            "#,
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
            result.output.as_bytes(),
            b"array(3) {\n  [0]=>\n  int(7)\n  [1]=>\n  int(8)\n  [2]=>\n  int(9)\n}\nint(7)\nArrayIterator::__construct(): Argument #1 ($array) must be of type array, int given\nNULL\n"
        );
}

#[test]
fn spl_temp_file_object_reports_empty_temp_stream_mvp() {
    let result = execute_source(
        "<?php $file = new SplTempFileObject(); echo $file->getPathname(), '|', $file->getSize(), '|', $file->eof() ? 'eof' : 'data';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"php://temp|0|data");
}

#[test]
fn foreach_executes_break_continue_and_nested_loops() {
    let flow = execute_source(
        "<?php foreach ([1, 2, 3, 4] as $value) { if ($value == 2) { continue; } if ($value == 4) { break; } echo $value; }",
    );
    assert!(flow.status.is_success(), "{:?}", flow.status);
    assert_eq!(flow.output.as_bytes(), b"13");

    let nested = execute_source(
        "<?php foreach ([\"a\", \"b\"] as $left) { foreach ([1, 2] as $right) { echo $left, $right, \";\"; } }",
    );
    assert!(nested.status.is_success(), "{:?}", nested.status);
    assert_eq!(nested.output.as_bytes(), b"a1;a2;b1;b2;");
}

#[test]
fn foreach_uses_snapshot_iteration_for_mutated_arrays() {
    let result = execute_source(
        "<?php $items = [1, 2]; foreach ($items as $value) { echo $value; $items[] = 9; } echo \"|\"; foreach ($items as $value) { echo $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"12|1299");
}

#[test]
fn foreach_object_properties_read_values_at_iteration_time() {
    let result = execute_source(
        "<?php class MutablePropsFixture { public $a = 1; public $b = 2; } $object = new MutablePropsFixture(); foreach ($object as $key => $value) { echo $key, \":\", $value, \";\"; if ($key === \"a\") { $object->b = 9; } } echo \"|\", $object->b;",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a:1;b:9;|9");
}

#[test]
fn foreach_by_ref_executes_local_array_and_lingering_reference() {
    let result = execute_source(
        "<?php $items = [1, 2]; foreach ($items as &$value) { $value = $value + 10; } unset($value); foreach ($items as $value) { echo $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1112");

    let lingering = execute_source(
        "<?php $items = [1, 2]; foreach ($items as &$value) { } $value = 9; echo $items[1];",
    );

    assert!(lingering.status.is_success(), "{:?}", lingering.status);
    assert_eq!(lingering.output.as_bytes(), b"9");
}

#[test]
fn foreach_by_ref_executes_key_value_and_appended_entries() {
    let key_value = execute_source(
        "<?php $items = [\"a\" => 1, \"b\" => 2]; foreach ($items as $key => &$value) { echo $key, \":\", $value, \";\"; $value = $value + 1; } unset($value); echo \"|\", $items[\"a\"], \":\", $items[\"b\"];",
    );

    assert!(key_value.status.is_success(), "{:?}", key_value.status);
    assert_eq!(key_value.output.as_bytes(), b"a:1;b:2;|2:3");

    let appended = execute_source(
        "<?php $items = [1, 2]; $done = false; foreach ($items as &$value) { echo $value; if (!$done) { $items[] = 3; $done = true; } } unset($value);",
    );

    assert!(appended.status.is_success(), "{:?}", appended.status);
    assert_eq!(appended.output.as_bytes(), b"123");
}

#[test]
fn foreach_by_ref_over_property_writes_back_to_property() {
    let result = execute_source(
        "<?php class C { public $items = [1, 2]; public function bump() { foreach ($this->items as &$value) { $value = $value + 1; } unset($value); } } $c = new C(); $c->bump(); foreach ($c->items as $value) { echo $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"23");
}

#[test]
fn foreach_by_ref_over_local_dim_writes_back_to_array_dimension() {
    let result = execute_source(
        "<?php $settings = ['blocks' => [['x' => 1], ['x' => 2]]]; foreach ($settings['blocks'] as &$block) { $block['x'] = $block['x'] + 10; } unset($block); foreach ($settings['blocks'] as $block) { echo $block['x'], ';'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"11;12;");
}

#[test]
fn nested_by_ref_foreach_unset_normalizes_rest_route_options() {
    let result = execute_source(
        "<?php class RestRouteFixture { public $endpoints = array(); public $route_options = array(); public function run() { $this->endpoints = array('/wp/v2/users/me' => array(0 => array('methods' => 'GET'), 1 => array('methods' => 'POST'), 2 => array('methods' => 'DELETE'), 'namespace' => 'wp/v2')); $endpoints = $this->endpoints; foreach ($endpoints as $route => &$handlers) { if (!isset($this->route_options[$route])) { $this->route_options[$route] = array(); } foreach ($handlers as $key => &$handler) { if (!is_numeric($key)) { $this->route_options[$route][$key] = $handler; unset($handlers[$key]); continue; } } } foreach ($endpoints['/wp/v2/users/me'] as $key => $handler) { echo $key, ':', gettype($handler), ';'; } echo '|', $this->route_options['/wp/v2/users/me']['namespace']; } } (new RestRouteFixture())->run();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0:array;1:array;2:array;|wp/v2");
}

#[test]
fn foreach_by_value_snapshots_reference_elements_without_aliasing() {
    let result = execute_source(
        "<?php $items = [1]; $alias =& $items[0]; foreach ($items as $value) { $value = 9; echo $items[0], \":\", $alias, \":\", $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1:1:9");
}

#[test]
fn foreach_by_ref_nonlocal_source_is_stable_known_gap() {
    let frontend = php_semantics::analyze_source("<?php foreach ([1] as &$value) { echo $value; }");
    let lowering = php_ir::lower_frontend_result(&frontend, php_ir::LoweringOptions::default());

    assert!(
        lowering.verification.is_ok(),
        "{:#?}",
        lowering.verification
    );
    assert_eq!(lowering.diagnostics.len(), 1);
    assert_eq!(
        lowering.diagnostics[0].id,
        "E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"
    );
    let result = Vm::new().execute(lowering.unit);
    assert_eq!(result.status.exit_status(), ExitStatus::Unsupported);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"
    );
}

#[test]
fn generator_call_is_lazy_and_foreach_runs_to_first_yield() {
    let result = execute_source(
        "<?php function gen() { echo 'body|'; yield 1; } $g = gen(); echo 'created|'; foreach ($g as $value) { echo 'v:', $value, '|'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"created|body|v:1|");
}

#[test]
fn generator_foreach_uses_key_and_value() {
    let result = execute_source(
        "<?php function gen() { yield 'a' => 7; } foreach (gen() as $key => $value) { echo $key, ':', $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a:7");
}

#[test]
fn generator_methods_use_same_state_handle() {
    let result = execute_source(
        "<?php function gen() { yield 'a' => 7; } $g = gen(); echo $g->valid() ? 'T' : 'F'; echo '|', $g->current(), '|', $g->key(); $g->next(); echo '|', $g->valid() ? 'T' : 'F';",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"T|7|a|F");
}

#[test]
fn generator_method_preserves_this_context() {
    let result = execute_source(
        "<?php class C { public $x = 'value'; public function gen() { yield $this->x; } } $c = new C(); foreach ($c->gen() as $value) { echo $value; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"value");
}

#[test]
fn generator_get_return_after_no_yield_completion() {
    let result = execute_source(
        "<?php function gen() { return 9; yield 1; } $g = gen(); $g->rewind(); echo $g->getReturn();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"9");
}

#[test]
fn generator_send_resumes_with_yield_expression_value() {
    let result = execute_source(
        "<?php function gen() { $value = yield 1; echo $value; } $g = gen(); $g->rewind(); $g->send(7);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7");
}

#[test]
fn generator_throw_injects_exception_at_suspend_point() {
    let result = execute_source(
        "<?php function gen() { try { yield 1; } catch (Exception $e) { echo $e->getMessage(); } } $g = gen(); $g->rewind(); $g->throw(new Exception('boom'));",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"boom");
}

#[test]
fn generator_foreach_resumes_to_return_value() {
    let result = execute_source(
        "<?php function gen() { yield 1; return 9; } $g = gen(); foreach ($g as $value) { echo $value, '|'; } echo $g->getReturn();",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|9");
}

#[test]
fn generator_yield_from_array_delegates_keys_and_values() {
    let result = execute_source(
        "<?php function gen() { yield from ['a' => 1, 'b' => 2]; } foreach (gen() as $key => $value) { echo $key, ':', $value, ';'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"a:1;b:2;");
}

#[test]
fn generator_yield_from_generator_returns_delegate_return_value() {
    let result = execute_source(
        "<?php function inner() { yield 'x' => 3; return 9; } function outer() { $result = yield from inner(); echo 'return:', $result; } foreach (outer() as $key => $value) { echo $key, ':', $value, '|'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"x:3|return:9");
}

#[test]
fn generator_yield_from_runs_finally_on_completion() {
    let result = execute_source(
        "<?php function gen() { try { yield from [1]; } finally { echo 'cleanup'; } } foreach (gen() as $value) { echo $value, '|'; }",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|cleanup");
}

#[test]
fn generator_get_return_before_completion_is_runtime_error() {
    let result = execute_source(
        "<?php function gen() { yield 1; return 9; } $g = gen(); $g->rewind(); echo $g->getReturn();",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_VM_GENERATOR_GET_RETURN_BEFORE_CLOSE"
    );
}

#[test]
fn normal_functions_are_not_treated_as_generators() {
    let result = execute_source("<?php function f() { return 3; } echo f();");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"3");
}

#[test]
fn eval_executes_code_and_returns_value() {
    let result =
        execute_source("<?php echo \"before|\", eval('echo \"inner|\"; return 7;'), \"|after\";");

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"before|inner|7|after");
}

#[test]
fn eval_shares_top_level_locals() {
    let result = execute_source(
        "<?php $message = \"parent\"; eval('$message = $message . \"|eval\";'); echo $message;",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"parent|eval");
}

#[test]
fn eval_returned_closures_keep_distinct_parameter_metadata() {
    let result = execute_source(
        "<?php $a = eval('return function($a) { echo $a; };'); $b = eval('return function($b) { echo $b; };'); $a(a: 1); echo '|'; $b(b: 2);",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"1|2");
}

#[test]
fn eval_class_declarations_merge_into_runtime_unit() {
    let result = execute_source(
        "<?php class C { const X = E::A; public static $a = array(K => D::V, E::A => K); } eval('class D extends C { const V = \"test\"; }'); class E extends D { const A = \"hello\"; } define('K', 'nasty'); var_dump(C::X, C::$a, D::X, D::$a, E::X, E::$a);",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(
            result.output.as_bytes(),
            b"string(5) \"hello\"\narray(2) {\n  [\"nasty\"]=>\n  string(4) \"test\"\n  [\"hello\"]=>\n  string(5) \"nasty\"\n}\nstring(5) \"hello\"\narray(2) {\n  [\"nasty\"]=>\n  string(4) \"test\"\n  [\"hello\"]=>\n  string(5) \"nasty\"\n}\nstring(5) \"hello\"\narray(2) {\n  [\"nasty\"]=>\n  string(4) \"test\"\n  [\"hello\"]=>\n  string(5) \"nasty\"\n}\n"
        );
}

#[test]
fn eval_parse_errors_are_runtime_diagnostics() {
    let result = execute_source("<?php eval('if (');");

    assert!(!result.status.is_success());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_VM_EVAL_PARSE_ERROR"),
        "diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn eval_function_declarations_are_visible_after_eval() {
    let result = execute_source(
        "<?php eval('function eval_runtime_fixture() { return 7; }'); echo eval_runtime_fixture();",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"7");
}

#[test]
fn eval_class_declarations_are_visible_after_eval() {
    let result = execute_source(
        "<?php eval('class EvalRuntimeFixture { const VALUE = 9; }'); echo EvalRuntimeFixture::VALUE;",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"9");
}

#[test]
fn eval_class_declarations_can_extend_existing_classes() {
    let result = execute_source(
        "<?php class EvalParentFixture { const VALUE = 'ok'; } eval('class EvalChildFixture extends EvalParentFixture {}'); echo EvalChildFixture::VALUE;",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn eval_class_declarations_are_visible_to_later_classes() {
    let result = execute_source(
        "<?php class EvalBaseForLaterClass { const VALUE = 'base'; } eval('class EvalMiddleForLaterClass extends EvalBaseForLaterClass {}'); class EvalLaterClass extends EvalMiddleForLaterClass { const OWN = 'own'; } echo EvalMiddleForLaterClass::VALUE, '|', EvalLaterClass::VALUE, '|', EvalLaterClass::OWN;",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"base|base|own");
}

#[test]
fn eval_duplicate_function_declarations_remain_fatal() {
    let result = execute_source(
        "<?php function eval_runtime_duplicate() {} eval('function eval_runtime_duplicate() {}');",
    );

    assert!(!result.status.is_success());
    assert!(
        result.diagnostics.iter().any(|diagnostic| diagnostic.id()
            == "E_PHP_VM_FUNCTION_REDECLARATION"
            && diagnostic.message().contains("Cannot redeclare function")),
        "{:#?}",
        result
    );
}

#[test]
fn eval_declarations_are_registered_for_later_runtime_lookup() {
    let result = execute_source(
        "<?php eval('function eval_runtime_fixture() { return 1; } class EvalRuntimeFixture {}'); echo eval_runtime_fixture(), '|', (new EvalRuntimeFixture())::class;",
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(result.output.as_bytes(), b"1|EvalRuntimeFixture");
}

#[test]
fn eval_concat_declaration_in_autoload_callback_registers_class() {
    let result = execute_source(
        r#"<?php
class X {
    public function getClosure() {
        return function($class) {
            echo "a2 called\n";
        };
    }
}
$a = function ($class) {
    echo "a called\n";
};
$x = new X;
$a2 = $x->getClosure();
$b = function ($class) {
    eval('class ' . $class . '{function __construct(){echo "foo\n";}}');
    echo "b called\n";
};
spl_autoload_register($a);
spl_autoload_register($a2);
spl_autoload_register($b);
$c = $a;
$c2 = $a2;
spl_autoload_register($c);
spl_autoload_register($c2);
$c = new foo;"#,
    );

    assert!(result.status.is_success(), "{:#?}", result);
    assert_eq!(
        result.output.as_bytes(),
        b"a called\na2 called\nb called\nfoo\n"
    );
}

#[test]
fn eval_recursion_limit_is_runtime_diagnostic() {
    let vm = Vm::new();
    let compiled = CompiledUnit::new(manual_return_unit(IrConstant::Null));
    let mut output = OutputBuffer::new();
    let mut stack = CallStack::new();
    let mut state = ExecutionState {
        eval_depth: MAX_EVAL_DEPTH,
        ..ExecutionState::default()
    };
    let result = vm.execute_eval(
        &compiled,
        &Value::string("echo \"done\";"),
        IrSpan::default(),
        &mut output,
        &mut stack,
        &mut state,
    );

    assert!(!result.status.is_success());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "E_PHP_VM_EVAL_RECURSION_LIMIT"),
        "diagnostics: {:#?}",
        result.diagnostics
    );
}

#[test]
fn unsupported_known_gaps_surface_stable_runtime_ids() {
    let diagnostic_ids = [
        "E_PHP_IR_UNSUPPORTED_GENERATOR",
        "E_PHP_IR_UNSUPPORTED_YIELD_FROM",
        "E_PHP_IR_UNSUPPORTED_FIBER",
        "E_PHP_IR_UNSUPPORTED_EVAL",
        "E_PHP_IR_UNSUPPORTED_AUTOLOAD",
        "E_PHP_IR_UNSUPPORTED_REFLECTION",
        "E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME",
        "E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME",
        "E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS",
        "E_PHP_IR_UNSUPPORTED_REFERENCE_SEMANTICS",
    ];

    for diagnostic_id in diagnostic_ids {
        let result = Vm::with_options(VmOptions {
            verify_ir: false,
            ..VmOptions::default()
        })
        .execute(manual_unsupported_unit_for(diagnostic_id));
        assert_eq!(
            result.status.exit_status(),
            ExitStatus::Unsupported,
            "{diagnostic_id}: {:?}",
            result.status
        );
        assert_eq!(result.diagnostics[0].id(), diagnostic_id);
    }
}

#[test]
fn builtins_execute_direct_calls_print_var_dump_and_callable_resolution() {
    let direct = execute_source(
        "<?php echo gettype(null), \"|\", gettype(7), \"|\", gettype(\"x\"), \"|\"; echo is_int(7), is_string(\"x\"), is_bool(false), is_null(null), is_array(null);",
    );
    assert!(direct.status.is_success(), "{:?}", direct.status);
    assert_eq!(direct.output.as_bytes(), b"NULL|integer|string|1111");

    let print = execute_source("<?php echo print \"x\";");
    assert!(print.status.is_success(), "{:?}", print.status);
    assert_eq!(print.output.as_bytes(), b"x1");

    let exit = execute_source("<?php echo \"before\\n\"; exit; echo \"after\\n\";");
    assert!(exit.status.is_success(), "{:?}", exit.status);
    assert_eq!(exit.output.as_bytes(), b"before\n");

    let short_circuit_or_die = execute_source("<?php false or die(\"failed\"); echo \"bad\";");
    assert!(
        short_circuit_or_die.status.is_success(),
        "{:?}",
        short_circuit_or_die.status
    );
    assert_eq!(short_circuit_or_die.output.as_bytes(), b"failed");
    assert_eq!(short_circuit_or_die.process_exit_code, Some(0));

    let short_circuit_or_skips_die = execute_source("<?php true or die(\"bad\"); echo \"after\";");
    assert!(
        short_circuit_or_skips_die.status.is_success(),
        "{:?}",
        short_circuit_or_skips_die.status
    );
    assert_eq!(short_circuit_or_skips_die.output.as_bytes(), b"after");

    let assignment_or_skips_die =
        execute_source("<?php $queue = \"queue\" or die(\"bad\"); echo $queue;");
    assert!(
        assignment_or_skips_die.status.is_success(),
        "{:?}",
        assignment_or_skips_die.status
    );
    assert_eq!(assignment_or_skips_die.output.as_bytes(), b"queue");

    let dump = execute_source(
        "<?php function dump_args(...$args) { var_dump($args); } var_dump(null, true, 7, \"hi\"); dump_args(1, \"x\");",
    );
    assert!(dump.status.is_success(), "{:?}", dump.status);
    assert_eq!(
        dump.output.to_string_lossy(),
        "NULL\nbool(true)\nint(7)\nstring(2) \"hi\"\narray(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"x\"\n}\n"
    );

    let callable =
        execute_source("<?php echo \"abc\" |> gettype(...), \"|\", \"abc\" |> strlen(...);");
    assert!(callable.status.is_success(), "{:?}", callable.status);
    assert_eq!(callable.output.as_bytes(), b"string|3");
}

#[test]
fn compact_builtin_reads_current_scope_locals() {
    let result = execute_source(
        "<?php
            function build_compact() {
                $charset = 'utf8mb4';
                $collate = 'utf8mb4_unicode_ci';
                $missing_names = ['missing'];
                $result = compact('charset', ['collate', $missing_names]);
                echo $result['charset'], '|', $result['collate'], '|',
                    array_key_exists('missing', $result) ? 'bad' : 'missing';
            }
            build_compact();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"utf8mb4|utf8mb4_unicode_ci|missing"
    );
}

#[test]
fn core_introspection_builtins_read_vm_request_state() {
    let result = execute_source(
        "<?php
            function handler() {}
            function inspect($param) {
                $local = 'value';
                $vars = get_defined_vars();
                echo $vars['param'], '|', $vars['local'], '|',
                    array_key_exists('missing', $vars) ? 'bad' : 'missing', \"\\n\";
            }
            inspect('arg');
            echo get_error_handler() === null ? 'no-error' : 'bad';
            set_error_handler('handler');
            echo '|', is_callable(get_error_handler()) ? 'error-handler' : 'bad';
            echo '|', get_exception_handler() === null ? 'no-exception' : 'bad';
            set_exception_handler('handler');
            echo '|', is_callable(get_exception_handler()) ? 'exception-handler' : 'bad';
            echo \"\\n\";
            $core = get_extension_funcs('core');
            echo in_array('zend_version', $core, true) ? 'core-func' : 'missing';
            echo '|', get_extension_funcs('missing') === false ? 'missing-ext' : 'bad';
            echo '|', function_exists('clone') && function_exists('die') && function_exists('exit')
                ? 'constructs' : 'missing-construct';
            echo '|', zend_version();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"arg|value|missing\nno-error|error-handler|no-exception|exception-handler\ncore-func|missing-ext|constructs|4.5.7"
    );
}

#[test]
fn included_files_builtins_expose_request_include_list() {
    let root =
        std::env::temp_dir().join(format!("phrust-vm-included-files-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    let main_path = root.join("main.php");
    let child_path = root.join("child.php");
    std::fs::write(&child_path, "<?php $child = 'ok';\n").expect("child should be writable");
    let source = "<?php
            require __DIR__ . '/child.php';
            $included = get_included_files();
            $required = get_required_files();
            echo basename($included[0]), '|', basename($included[1]), '|',
                count($included), '|', count($required), '|', $child;
        ";
    std::fs::write(&main_path, source).expect("main should be writable");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            ..VmOptions::default()
        },
        main_path.to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"main.php|child.php|2|2|ok");
}

#[test]
fn array_change_key_case_transforms_string_keys() {
    let result = execute_source(
        "<?php
            $lower = array_change_key_case(['Mixed' => 1, 7 => 2]);
            $upper = array_change_key_case(['mixed' => 3], 1);
            echo $lower['mixed'], '|', $lower[7], '|', $upper['MIXED'];
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1|2|3");
}

#[test]
fn array_callback_builtins_execute_php_callables() {
    let result = execute_source(
            "<?php
            function plus_one($v) { return $v + 1; }
            class Scale { static function double($v) { return $v * 2; } }
            $input = ['a' => 1, 'b' => 2, 'c' => 3];
            echo var_export(array_map('plus_one', $input), true), \"\\n\";
            echo var_export(array_map(['Scale', 'double'], [1, 2]), true), \"\\n\";
            echo var_export(array_filter($input, fn($v, $k) => $v > 1 && $k !== 'c', 1), true), \"\\n\";
            echo array_reduce([1, 2, 3], fn($carry, $v) => $carry + $v, 0), \"\\n\";
            $walk = ['x' => 1, 'y' => 2];
            array_walk($walk, function($v, $k) { echo $k, ':', $v, ';'; });
            echo \"\\n\";
            echo array_any($input, fn($v, $k) => $k === 'b') ? 'T' : 'F';
            echo array_all($input, fn($v, $k) => $v > 0) ? 'T' : 'F';
            echo '|', array_find($input, fn($v, $k) => $v === 2);
            echo '|', array_find_key($input, fn($v, $k) => $v === 3);
            ",
        );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array (\n  'a' => 2,\n  'b' => 3,\n  'c' => 4,\n)\narray (\n  0 => 2,\n  1 => 4,\n)\narray (\n  'b' => 2,\n)\n6\nx:1;y:2;\nTT|2|c"
    );
}

#[test]
fn array_walk_accepts_objects_and_preserves_php_property_keys() {
    let stdclass = execute_source(
        "<?php
            $object = new stdclass;
            $object->foo = 'foo';
            $object->bar = 'bar';
            array_walk($object, function($value, $key) { echo $key, ':', $value, '|'; });
            ",
    );
    assert!(stdclass.status.is_success(), "{:?}", stdclass.status);
    assert_eq!(stdclass.output.as_bytes(), b"foo:foo|bar:bar|");

    let declared = execute_source(
        "<?php
            class WalkBox {
                private $pri = 'private';
                public $pub = 'public';
                protected $pro = 'protected';
            }
            $declaredObject = new WalkBox();
            array_walk($declaredObject, function($value, $key) { echo $key, ':', $value, '|'; });
            ",
    );
    assert!(declared.status.is_success(), "{:?}", declared.status);
    assert_eq!(
        declared.output.as_bytes(),
        b"\0WalkBox\0pri:private|pub:public|\0*\0pro:protected|"
    );
}

#[test]
fn array_walk_callbacks_receive_element_references() {
    let result = execute_source(
        "<?php
            $flat = ['x' => 1, 'y' => 2];
            array_walk($flat, function(&$value, $key) { $value += 10; });
            echo var_export($flat, true), \"\\n\";
            $nested = ['a' => ['b' => 'z'], 'c' => 'q'];
            array_walk_recursive($nested, function(&$value) { $value = strtoupper($value); });
            echo var_export($nested, true);
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array (\n  'x' => 11,\n  'y' => 12,\n)\narray (\n  'a' => \n  array (\n    'b' => 'Z',\n  ),\n  'c' => 'Q',\n)"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.id() != "E_PHP_VM_BY_REF_ARG_VALUE_GIVEN_WARNING")
    );
}

#[test]
fn new_self_in_static_method_uses_declaring_class() {
    let result = execute_source(
        "<?php
            class C {
                private static $instance = null;
                public static function get_instance() {
                    if ( null === self::$instance ) {
                        self::$instance = new self();
                    }
                    return self::$instance;
                }
            }
            echo get_class(C::get_instance());
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"C");
}

#[test]
fn array_walk_recursive_walks_nested_arrays_and_reports_type_errors() {
    let nested = execute_source(
        "<?php
            $nested = ['a' => ['b' => 1], 'c' => 2];
            array_walk_recursive($nested, function($value, $key) {
                echo $key, ':', $value, '|';
            });
            ",
    );
    assert!(nested.status.is_success(), "{:?}", nested.status);
    assert_eq!(nested.output.as_bytes(), b"b:1|c:2|");

    let type_error = execute_source(
        "<?php
            try {
                $notArray = '';
                array_walk($notArray, function() {});
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            ",
    );
    assert!(type_error.status.is_success(), "{:?}", type_error.status);
    assert_eq!(
        type_error.output.to_string_lossy(),
        "array_walk(): Argument #1 ($array) must be of type array, string given"
    );
}

#[test]
fn array_sort_builtins_mutate_arrays_and_call_comparators() {
    let result = execute_source(
        "<?php
            $a = [2 => 'b', 0 => 'a', 1 => 'c'];
            sort($a);
            echo var_export($a, true), \"\\n\";
            $b = ['z' => 2, 'a' => 1, 'm' => 3];
            asort($b);
            echo var_export($b, true), \"\\n\";
            krsort($b);
            echo var_export($b, true), \"\\n\";
            $c = [3, 1, 2];
            usort($c, fn($left, $right) => $right <=> $left);
            echo var_export($c, true), \"\\n\";
            $d = ['img10', 'img2', 'img1'];
            natsort($d);
            echo var_export($d, true), \"\\n\";
            $e = ['B', 'a'];
            sort($e, SORT_STRING | SORT_FLAG_CASE);
            echo var_export($e, true), \"\\n\";
            $f = ['B' => 1, 'a' => 2];
            ksort($f, SORT_STRING | SORT_FLAG_CASE);
            echo var_export($f, true), \"\\n\";
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "array (\n  0 => 'a',\n  1 => 'b',\n  2 => 'c',\n)\narray (\n  'a' => 1,\n  'z' => 2,\n  'm' => 3,\n)\narray (\n  'z' => 2,\n  'm' => 3,\n  'a' => 1,\n)\narray (\n  0 => 3,\n  1 => 2,\n  2 => 1,\n)\narray (\n  2 => 'img1',\n  1 => 'img2',\n  0 => 'img10',\n)\narray (\n  0 => 'a',\n  1 => 'B',\n)\narray (\n  'a' => 2,\n  'B' => 1,\n)\n"
    );
}

#[test]
fn array_sort_builtins_mutate_array_dimensions() {
    let result = execute_source(
        "<?php
            function cmp($left, $right) { return $left <=> $right; }
            $arrays = [[2, 10, -1], [100], [], [0], [-1], [-9, 34, 54, 0, 20]];
            var_dump(usort($arrays[5], 'cmp'));
            echo var_export($arrays[5], true);
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\narray (\n  0 => -9,\n  1 => 0,\n  2 => 20,\n  3 => 34,\n  4 => 54,\n)"
    );
}

#[test]
fn by_ref_builtins_mutate_array_dimensions() {
    let result = execute_source(
        "<?php
            $arrays = [[1, 2, 3]];
            var_dump(shuffle($arrays[0]));
            echo count($arrays[0]), '|', array_is_list($arrays[0]) ? 'list' : 'not-list';
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "bool(true)\n3|list");
}

#[test]
fn by_ref_builtins_mutate_property_array_dimensions() {
    let result = execute_source(
        "<?php
            class C {
                public $iterations;
                function __construct() { $this->iterations = [[1, 2]]; }
                function run($i) {
                    var_dump(next($this->iterations[$i]));
                    var_dump(next($this->iterations[$i]));
                }
            }
            $c = new C;
            $c->run(0);
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "int(2)\nbool(false)\n");
}

#[test]
fn by_ref_builtin_direct_temporary_is_fatal_error() {
    let result = execute_source("<?php var_dump(prev(array(1, 2)));");

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
            output.contains(
                "Fatal error: Uncaught Error: prev(): Argument #1 ($array) could not be passed by reference"
            ),
            "{output}"
        );
    assert!(
        output.contains("Stack trace:\n#0 {main}\n  thrown in "),
        "{output}"
    );
    assert!(!output.contains("<unknown>:0"), "{output}");
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_VM_INTERNAL_BY_REF_ARG_NOT_REFERENCEABLE"
    );
}

#[test]
fn by_ref_builtin_indirect_temporary_warns_and_uses_temp_cell() {
    let result = execute_source(
        "<?php
            function f() {
                $array = array(1, 2);
                end($array);
                return $array;
            }
            var_dump(prev(f()));
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "\nNotice: Only variables should be passed by reference in <unknown> on line 0\nint(1)\n"
    );
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.id() == "E_PHP_VM_BY_REF_ARG_INDIRECT_TEMPORARY_NOTICE"
        })
    );
}

#[test]
fn openssl_random_pseudo_bytes_initializes_strong_result_output_arg() {
    let result = execute_source(
        "<?php
            $bytes = openssl_random_pseudo_bytes(8, $strong);
            echo strlen($bytes), '|', $strong ? 'strong' : 'weak';
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"8|strong");
}

#[test]
fn pcre_replace_builtins_initialize_count_output_args_quietly() {
    let result = execute_source(
        "<?php
            echo preg_replace('/a/', 'b', 'aa', -1, $replace_count), ':', $replace_count, \"\\n\";
            echo preg_filter('/a/', 'b', 'aa', -1, $filter_count), ':', $filter_count, \"\\n\";
            echo preg_replace_callback('/a/', fn($m) => 'b', 'aa', -1, $callback_count), ':', $callback_count, \"\\n\";
            echo preg_replace_callback_array(['/a/' => fn($m) => 'b'], 'aa', -1, $array_count), ':', $array_count;
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"bb:2\nbb:2\nbb:2\nbb:2");
}

#[test]
fn array_sort_builtins_mutate_private_properties() {
    let result = execute_source(
        "<?php
            class Box {
                private $values = [2, 1];
                public function run() {
                    sort($this->values);
                    echo implode(',', $this->values);
                }
            }
            (new Box())->run();
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1,2");
}

#[test]
fn array_sort_callbacks_can_call_private_methods_from_current_scope() {
    let result = execute_source(
        "<?php
            class Box {
                private $values = ['b' => [2], 'a' => [1]];
                private function cmp($left, $right) {
                    if (!isset($this->values[$left])) {
                        throw new Exception('missing left');
                    }
                    if (!isset($this->values[$right])) {
                        throw new Exception('missing right');
                    }
                    return $left <=> $right;
                }
                public function run() {
                    uksort($this->values, [$this, 'cmp']);
                    echo 'Done';
                }
            }
            (new Box())->run();
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"Done");
}

#[test]
fn array_sort_bool_comparators_deprecate_and_use_reverse_compare() {
    let result = execute_source(
        "<?php
            function bool_cmp($left, $right) { return $left > $right; }
            $values = [2, 0, 1];
            usort($values, 'bool_cmp');
            echo '|', implode(',', $values);
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output
            .contains("Deprecated: usort(): Returning bool from comparison function is deprecated"),
        "{output}"
    );
    assert!(output.ends_with("|0,1,2"), "{output}");
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.id() == "E_PHP_VM_SORT_BOOL_COMPARE_DEPRECATED"
            && diagnostic.severity() == RuntimeSeverity::Deprecation
    }));
}

#[test]
fn dynamic_string_calls_dispatch_array_sort_builtins_in_vm() {
    let result = execute_source(
        "<?php
            $sort = 'sort';
            $values = [3, 1, 2];
            $sort($values);
            echo implode(',', $values);
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1,2,3");
}

#[test]
fn sort_string_warns_for_array_to_string_values() {
    let result = execute_source(
        "<?php
            $values = [[1], 'b'];
            sort($values, SORT_STRING);
            echo '|done';
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Warning: Array to string conversion in "),
        "{output}"
    );
    assert!(output.ends_with("|done"), "{output}");
}

#[test]
fn array_multisort_string_cast_warnings_respect_error_handler() {
    let result = execute_source(
        "<?php
            function ignore_sort_warning($errno, $errstr) {}
            set_error_handler('ignore_sort_warning');
            $inputs = [
                'int 0' => 0,
                [],
                'uppercase NULL' => NULL,
                'empty string DQ' => '',
                'string DQ' => 'string',
            ];
            var_dump(array_multisort($inputs, SORT_STRING));
            foreach ($inputs as $key => $_) {
                echo $key;
                break;
            }
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        !output.contains("Warning: Array to string conversion"),
        "{output}"
    );
    assert_eq!(output, "bool(true)\nuppercase NULL");
}

#[test]
fn array_multisort_regular_orders_mixed_values_like_php() {
    let result = execute_source(
        "<?php
            class SortWithToString {
                public function __toString(): string {
                    return 'Class A object';
                }
            }
            class SortWithoutToString {}
            $inputs = [
                'int 0' => 0,
                'float -10.5' => -10.5,
                [],
                'uppercase NULL' => NULL,
                'lowercase true' => true,
                'empty string DQ' => '',
                'string DQ' => 'string',
                'with' => new SortWithToString(),
                'without' => new SortWithoutToString(),
                'undefined var' => @$undefined_var,
            ];
            var_dump(array_multisort($inputs));
            foreach ($inputs as $key => $_) {
                echo $key, \"\\n\";
            }
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nfloat -10.5\nint 0\n0\nuppercase NULL\nempty string DQ\nundefined var\nlowercase true\nwith\nstring DQ\nwithout\n"
    );
}

#[test]
fn sort_regular_compares_objects_by_properties_before_to_string() {
    let result = execute_source(
        "<?php
            class SortObjectValue {
                public $class_value;
                function __construct($value) {
                    $this->class_value = $value;
                }
                function __toString() {
                    return '';
                }
            }
            $values = [
                new SortObjectValue('axx'),
                new SortObjectValue('t'),
                new SortObjectValue('w'),
                new SortObjectValue('py'),
                new SortObjectValue('apple'),
                new SortObjectValue('Orange'),
                new SortObjectValue('Lemon'),
                new SortObjectValue('aPPle'),
            ];
            var_dump(sort($values));
            foreach ($values as $value) {
                echo $value->class_value, \"\\n\";
            }
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\nLemon\nOrange\naPPle\napple\naxx\npy\nt\nw\n"
    );
}

#[test]
fn array_multisort_mutates_multiple_arrays_with_flags() {
    let result = execute_source(
        "<?php
            $ar1 = ['row1' => 2, 'row2' => 1, 'row3' => 1];
            $ar2 = ['row1' => 2, 'row2' => 'aa', 'row3' => '1'];
            var_dump(array_multisort($ar1, SORT_ASC, SORT_REGULAR, $ar2, SORT_DESC, SORT_STRING));
            echo var_export($ar1, true), \"\\n\", var_export($ar2, true), \"\\n\";
            var_dump(array_multisort($ar2));
            echo var_export($ar2, true), \"\\n\";
            var_dump(array_multisort([1, 3, 2, 4]));
            try {
                array_multisort($ar2, SORT_ASC, SORT_ASC);
            } catch (TypeError $e) {
                echo $e->getMessage(), \"\\n\";
            }
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\narray (\n  'row2' => 1,\n  'row3' => 1,\n  'row1' => 2,\n)\narray (\n  'row2' => 'aa',\n  'row3' => '1',\n  'row1' => 2,\n)\nbool(true)\narray (\n  'row3' => '1',\n  'row1' => 2,\n  'row2' => 'aa',\n)\nbool(true)\narray_multisort(): Argument #3 must be an array or a sort flag that has not already been specified\n"
    );
}

#[test]
fn arsort_regular_orders_arrays_before_strings() {
    let result = execute_source(
        "<?php
            $values = [
                'array1' => [],
                'array2' => [1],
                'b' => 'b',
                'ab' => 'ab',
                4 => 4.01,
                0 => 0.001,
            ];
            var_dump(arsort($values));
            echo var_export(array_keys($values), true), \"\\n\";
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "bool(true)\narray (\n  0 => 'array2',\n  1 => 'array1',\n  2 => 'b',\n  3 => 'ab',\n  4 => 4,\n  5 => 0,\n)\n"
    );
}

#[test]
fn call_by_ref_param_mutates_caller_local() {
    let result = execute_source(
        "<?php function inc_ref(&$x) { $x = $x + 1; } $a = 1; inc_ref($a); echo $a;",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"2");
}

#[test]
fn call_by_ref_return_binds_to_caller_local() {
    let result = execute_source(
        "<?php function &identity_ref(&$x) { return $x; } $a = 1; $b =& identity_ref($a); $b = 4; echo $a, '|', $b;",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"4|4");
}

#[test]
fn call_by_ref_method_return_executes() {
    let result = execute_source(
        "<?php class C { public function &counter() { static $x = 0; return $x; } } $c = new C(); echo $c->counter();",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"0");
}

#[test]
fn call_by_ref_errors_for_temporaries() {
    let arg =
        execute_source("<?php function inc_ref(&$x) { $x = $x + 1; } $a = 1; inc_ref($a + 1);");
    assert_eq!(arg.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(arg.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        arg.diagnostics[0]
            .message()
            .contains("inc_ref(): Argument #1 ($x) could not be passed by reference"),
        "{:?}",
        arg.diagnostics[0]
    );

    let ret = execute_source("<?php function &bad_ref() { return 1; } $x =& bad_ref();");
    assert_eq!(ret.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(ret.diagnostics[0].id(), "E_PHP_VM_BY_REF_RETURN_TEMPORARY");

    let auto_ret = execute_source_with_options(
        "<?php function &bad_ref() { return 1; } $x =& bad_ref();",
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            ..VmOptions::default()
        },
    );
    assert_eq!(auto_ret.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        auto_ret.diagnostics[0].id(),
        "E_PHP_VM_BY_REF_RETURN_TEMPORARY"
    );
}

#[test]
fn call_by_ref_class_constant_uses_caller_only_trace() {
    let result = execute_source(
        "<?php
            class C { const VALUE = 1; }
            function take_ref(&$value) {}
            take_ref(C::VALUE);
            ",
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let output = result.output.to_string_lossy();
    assert!(
            output.contains(
                "Fatal error: Uncaught Error: take_ref(): Argument #1 ($value) could not be passed by reference"
            ),
            "{output}"
        );
    assert!(output.contains("Stack trace:\n#0 {main}"), "{output}");
    assert!(!output.contains("): take_ref()"), "{output}");
}

#[test]
fn call_by_ref_argument_mismatch_is_catchable_error() {
    let result = execute_source(
        "<?php
            function set_value(&$value): void { $value = 2; }
            try {
                set_value(1);
            } catch (Error $e) {
                echo 'by-ref';
            }
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"by-ref");
}

#[test]
fn sysvshm_destroyed_handle_is_catchable_error() {
    let key = unique_sysvshm_vm_key(1);
    let source = r"<?php
            $shm = shm_attach(__KEY__, 1024);
            shm_remove($shm);
            shm_detach($shm);
            try {
                shm_remove($shm);
            } catch (Error $e) {
                echo $e->getMessage();
            }
            "
    .replace("__KEY__", &key.to_string());
    let result = execute_source(&source);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"Shared memory block has already been destroyed"
    );
}

#[test]
fn sysvshm_put_var_propagates_serialize_exception() {
    let key = unique_sysvshm_vm_key(2);
    let source = r"<?php
            $shm = shm_attach(__KEY__, 1024);
            class SysvshmSerializeThrows {
                public function __serialize(): array {
                    throw new Error('no');
                }
            }
            try {
                shm_put_var($shm, 1, new SysvshmSerializeThrows);
            } catch (Error $e) {
                echo $e->getMessage();
            }
            shm_remove($shm);
            "
    .replace("__KEY__", &key.to_string());
    let result = execute_source(&source);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"no");
}

#[test]
fn sysvshm_put_var_detects_detach_during_serialize() {
    let key = unique_sysvshm_vm_key(3);
    let source = r"<?php
            class SysvshmSerializeDetaches {
                public function __serialize(): array {
                    global $shm;
                    shm_remove($shm);
                    shm_detach($shm);
                    return ['a' => 'b'];
                }
            }
            $shm = shm_attach(__KEY__, 1024);
            try {
                shm_put_var($shm, 1, new SysvshmSerializeDetaches);
            } catch (Error $e) {
                echo $e->getMessage();
            }
            "
    .replace("__KEY__", &key.to_string());
    let result = execute_source(&source);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"Shared memory block has been destroyed by the serialization function"
    );
}

fn unique_sysvshm_vm_key(offset: i64) -> i64 {
    0x5500_0000_i64 | (((std::process::id() as i64) & 0xffff) << 4) | (offset & 0x0f)
}

#[test]
fn sysvmsg_send_propagates_serialize_return_type_error() {
    let result = execute_source(
        "<?php
            class SysvmsgSerializeInvalid {
                public function __serialize() {}
            }
            $queue = msg_get_queue(45);
            try {
                msg_send($queue, 1, new SysvmsgSerializeInvalid, true);
            } catch (TypeError $e) {
                echo $e->getMessage();
            }
            msg_remove_queue($queue);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"SysvmsgSerializeInvalid::__serialize() must return an array"
    );
}

#[test]
fn sysvmsg_receive_size_value_error_is_catchable() {
    let result = execute_source(
        "<?php
            $queue = msg_get_queue(46);
            try {
                msg_receive($queue, 0, $type, 0, $message);
            } catch (ValueError $exception) {
                echo $exception->getMessage();
            }
            msg_remove_queue($queue);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"msg_receive(): Argument #4 ($max_message_size) must be greater than 0"
    );
}

#[test]
fn sysvmsg_send_removed_queue_emits_warning_and_sets_errno() {
    let result = execute_source(
        "<?php
            $queue = msg_get_queue(47);
            msg_remove_queue($queue);
            var_dump(msg_send($queue, 1, 'payload', true, true, $errno));
            var_dump($errno !== 0);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(
        output.contains("Warning: msg_send(): msgsnd failed: Invalid argument in "),
        "{output}"
    );
    assert!(output.contains("bool(false)\nbool(true)\n"), "{output}");
}

#[test]
fn call_user_func_array_by_ref_value_warns_and_calls() {
    let result = execute_source(
        "<?php
            function needs_ref(&$value): void { echo 'called'; }
            call_user_func_array('needs_ref', [1]);
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Warning: needs_ref():"), "{output}");
    assert!(output.contains("value given"), "{output}");
    assert!(output.ends_with("called"), "{output}");
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.id() == "E_PHP_VM_BY_REF_ARG_VALUE_GIVEN_WARNING"
            && diagnostic.severity() == RuntimeSeverity::Warning
    }));
}

#[test]
fn list_destructuring_holes_use_source_offsets() {
    let result = execute_source("<?php [$first, , $third] = [1, 2, 3]; echo $first, ':', $third;");

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"1:3");
}

#[test]
fn dynamic_static_call_uses_variable_method_value() {
    let result = execute_source(
        "<?php
            class DynamicStaticProbe { public static function label(): string { return 'ok'; } }
            $class = DynamicStaticProbe::class;
            $method = 'label';
            echo $class::$method();
            ",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
}

#[test]
fn array_sort_callbacks_warn_for_by_ref_value_arguments() {
    let result = execute_source(
        "<?php
            $values = ['b' => 'Banana', 'm' => 'Mango', 'a' => 'Apple'];
            uasort($values, function (&$left, &$right) {
                return $left <=> $right;
            });
            echo implode(',', array_keys($values));
            ",
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    let output = result.output.to_string_lossy();
    assert!(output.contains("Warning: {closure:"));
    assert!(output.contains("Argument #1 ($left) must be passed by reference, value given"));
    assert!(output.contains("Argument #2 ($right) must be passed by reference, value given"));
    assert!(output.ends_with("a,b,m"));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.id() == "E_PHP_VM_BY_REF_ARG_VALUE_GIVEN_WARNING"
            && diagnostic.severity() == RuntimeSeverity::Warning
    }));
}

#[test]
fn runtime_errors_emit_structured_diagnostics_and_warning_continuation() {
    let division = execute_source("<?php echo 1 / 0;");
    assert_eq!(division.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(division.status.message(), Some("division by zero"));
    assert_eq!(division.diagnostics.len(), 1);
    assert_eq!(
        division.diagnostics[0].id(),
        "E_PHP_RUNTIME_DIVISION_BY_ZERO"
    );
    assert_eq!(division.diagnostics[0].stack_trace()[0].function(), "main");

    // Undefined functions throw a catchable Error; uncaught it surfaces as
    // the uncaught-exception diagnostic with the reference wording.
    let undefined = execute_source("<?php missing_function();");
    assert_eq!(undefined.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(undefined.diagnostics[0].id(), "E_PHP_VM_UNCAUGHT_EXCEPTION");
    assert!(
        undefined.diagnostics[0]
            .message()
            .contains("Uncaught Error: Call to undefined function missing_function()"),
        "{}",
        undefined.diagnostics[0].message()
    );

    let stack = execute_source("<?php function boom() { echo 1 / 0; } boom();");
    assert_eq!(stack.status.exit_status(), ExitStatus::RuntimeError);
    let frames = stack.diagnostics[0]
        .stack_trace()
        .iter()
        .map(|frame| frame.function())
        .collect::<Vec<_>>();
    assert_eq!(frames, vec!["boom", "main"]);

    let warning = execute_source("<?php echo $missing, \"ok\";");
    assert!(warning.status.is_success(), "{:?}", warning.status);
    let warning_output = warning.output.to_string_lossy();
    assert!(warning_output.contains("Warning: Undefined variable $missing in "));
    assert!(warning_output.contains(" on line "));
    assert!(warning_output.ends_with("ok"));
    assert_eq!(warning.diagnostics.len(), 1);
    assert_eq!(
        warning.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNDEFINED_VARIABLE_WARNING"
    );
    assert_eq!(warning.diagnostics[0].severity(), RuntimeSeverity::Warning);

    let deprecated_interpolation = execute_source("<?php $name = 'P'; echo \"${name}HP\";");
    assert!(
        deprecated_interpolation.status.is_success(),
        "{:?}",
        deprecated_interpolation.status
    );
    let deprecated_output = deprecated_interpolation.output.to_string_lossy();
    assert!(
        deprecated_output
            .contains("Deprecated: Using ${var} in strings is deprecated, use {$var} instead in "),
        "{deprecated_output}"
    );
    assert!(deprecated_output.ends_with("PHP"), "{deprecated_output}");
    assert_eq!(deprecated_interpolation.diagnostics.len(), 1);
    assert_eq!(
        deprecated_interpolation.diagnostics[0].id(),
        "E_PHP_RUNTIME_DEPRECATED_DOLLAR_BRACE_INTERPOLATION"
    );
    assert_eq!(
        deprecated_interpolation.diagnostics[0].severity(),
        RuntimeSeverity::Deprecation
    );

    let leading_numeric = execute_source("<?php var_dump(1 + '2x');");
    assert!(
        leading_numeric.status.is_success(),
        "{:?}",
        leading_numeric.status
    );
    let leading_numeric_output = leading_numeric.output.to_string_lossy();
    assert!(
        leading_numeric_output.starts_with(
            "\nWarning: A non-numeric value encountered in /tmp/phrust-test.php on line "
        ),
        "{leading_numeric_output}"
    );
    assert!(leading_numeric_output.ends_with("\nint(3)\n"));
    assert_eq!(
        leading_numeric.diagnostics[0].id(),
        "E_PHP_RUNTIME_NON_NUMERIC_STRING_WARNING"
    );
    assert_eq!(
        leading_numeric.diagnostics[0].severity(),
        RuntimeSeverity::Warning
    );

    let array_to_string = execute_source("<?php echo [1, 2], \"\\ndone\";");
    assert!(
        array_to_string.status.is_success(),
        "{:?}",
        array_to_string.status
    );
    let array_to_string_output = array_to_string.output.to_string_lossy();
    assert!(array_to_string_output.contains("Warning: Array to string conversion in "));
    assert!(array_to_string_output.contains(" on line "));
    assert!(array_to_string_output.ends_with("Array\ndone"));
    assert_eq!(
        array_to_string.diagnostics[0].id(),
        "E_PHP_RUNTIME_ARRAY_TO_STRING_WARNING"
    );
    assert_eq!(
        array_to_string.diagnostics[0].severity(),
        RuntimeSeverity::Warning
    );

    let unsupported = Vm::with_options(VmOptions {
        verify_ir: false,
        ..VmOptions::default()
    })
    .execute(manual_unsupported_unit());
    assert_eq!(unsupported.status.exit_status(), ExitStatus::Unsupported);
    assert_eq!(unsupported.diagnostics.len(), 1);
    assert_eq!(
        unsupported.diagnostics[0].id(),
        "E_PHP_RUNTIME_UNSUPPORTED_GENERATOR_EXECUTION"
    );
    assert_eq!(
        unsupported.diagnostics[0].severity(),
        RuntimeSeverity::UnsupportedFeature
    );
}

#[test]
fn execution_deadline_reports_stable_timeout_diagnostic() {
    let result = execute_source_with_options(
        "<?php while (true) { }",
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli("timeout.php", Vec::new())
                .with_execution_time_limit(Some(Duration::ZERO)),
            max_steps: 1_000_000,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_EXECUTION_TIMEOUT");
    assert!(
        result
            .status
            .message()
            .is_some_and(|message| message.contains("maximum execution time exceeded")),
        "{:?}",
        result.status
    );
}

#[test]
fn dense_bytecode_auto_mixes_dense_functions_and_rich_fallback_functions() {
    let result = execute_source_with_options(
        r#"<?php
class LocalBox {
    public $value = 7;
}
function dense_supported($seed) {
    $items = [1, 2, $seed];
    return $items[2] + 3;
}
function rich_fallback() {
    $class = 'LocalBox';
    $box = new $class();
    return $box->value;
}
echo dense_supported(4), "\n", rich_fallback(), "\n";
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "7\n7\n");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 1, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.dense_functions_planned >= 2, "{counters:?}");
    assert!(counters.dense_functions_executed >= 2, "{counters:?}");
    assert!(
        counters.rich_fallback_functions_planned >= 1,
        "{counters:?}"
    );
    assert_eq!(counters.rich_fallback_functions_executed, 1, "{counters:?}");
    assert_eq!(
        counters
            .dense_function_fallback_by_reason
            .get("object_instantiation"),
        Some(&1),
        "{counters:?}"
    );
    assert_eq!(
        counters
            .rich_fallback_functions_by_name
            .get("rich_fallback"),
        Some(&1),
        "{counters:?}"
    );
    assert!(
        counters
            .dense_instruction_families_executed
            .get("function_calls")
            .copied()
            .unwrap_or_default()
            >= 2,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_falls_back_for_by_ref_return_functions() {
    let result = execute_source_with_options(
        "<?php function &bad_ref() { return 1; } $x =& bad_ref();",
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    assert_eq!(
        result.diagnostics[0].id(),
        "E_PHP_VM_BY_REF_RETURN_TEMPORARY"
    );
}

#[test]
fn dense_bytecode_auto_executes_direct_nested_function_declare() {
    let result = execute_source_with_options(
        r#"<?php
function outer_direct_nested() {
    function inner_direct_nested() {
        return 'declared';
    }
    return inner_direct_nested();
}
echo function_exists('inner_direct_nested') ? 'early' : 'missing';
echo '|', outer_direct_nested();
echo '|', function_exists('inner_direct_nested') ? 'late' : 'missing';
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "missing|declared|late");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 1, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    // The nested `function inner_direct_nested()` declaration now lowers and
    // runs on the dense bytecode path, so the enclosing function no longer
    // deopts to the rich interpreter for the declaration.
    assert!(
        counters
            .dense_instruction_families_executed
            .get("declarations")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
    assert!(
        !counters
            .dense_call_fallback_by_reason
            .keys()
            .any(|reason| reason.contains("DeclareFunction")),
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_include() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-dense-include-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(root.join("lib.php"), "<?php echo 'lib'; return 7;\n")
        .expect("include file should be written");
    let source = "<?php $first = require 'lib.php'; $second = require 'lib.php'; echo '|', $first + $second;";
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            dense_include_execution: DenseIncludeMode::Auto,
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::new(IncludeCache::new_with_revalidation_interval(
                1,
                std::time::Duration::ZERO,
            ))),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"liblib|14");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 2, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 2, "{counters:?}");
    assert!(
        counters.dense_execution_plan_cache_hits >= 1,
        "{counters:?}"
    );
    assert!(
        counters.dense_execution_plan_cache_misses >= 2,
        "{counters:?}"
    );
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.bytecode_instructions_executed >= 1, "{counters:?}");
    assert!(
        counters.entry_bytecode_instructions_executed >= 1,
        "{counters:?}"
    );
    assert!(
        counters.include_bytecode_instructions_executed >= 1,
        "{counters:?}"
    );
    assert_eq!(counters.dense_include_entry_attempts, 2, "{counters:?}");
    assert_eq!(counters.dense_include_entry_successes, 2, "{counters:?}");
    assert_eq!(counters.dense_include_entry_fallbacks, 0, "{counters:?}");
    assert!(counters.dense_functions_executed >= 1, "{counters:?}");
    assert!(counters.includes >= 1, "{counters:?}");
    assert!(
        counters
            .dense_instruction_families_executed
            .get("includes")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn dense_include_exports_assigned_locals_to_function_scope() {
    let root = std::env::temp_dir().join(format!(
        "phrust-vm-dense-include-function-scope-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp include root should be created");
    std::fs::write(
        root.join("version.php"),
        "<?php $app_version = '7.0-src';\n",
    )
    .expect("include file should be written");
    let source = r#"<?php
function version_probe() {
    require 'version.php';
    echo $app_version;
}
version_probe();
"#;
    std::fs::write(root.join("index.php"), source).expect("entry source should be written");

    let result = execute_source_with_options_and_path(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            dense_include_execution: DenseIncludeMode::Auto,
            include_loader: Some(IncludeLoader::for_root(&root).expect("loader")),
            include_cache: Some(Arc::new(IncludeCache::new_with_revalidation_interval(
                1,
                std::time::Duration::ZERO,
            ))),
            runtime_context: RuntimeContext::default().with_cwd(root.clone()),
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
        root.join("index.php").to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_dir_all(&root);

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"7.0-src");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.dense_include_entry_attempts, 1, "{counters:?}");
    assert_eq!(counters.dense_include_entry_successes, 1, "{counters:?}");
    assert_eq!(counters.dense_include_entry_fallbacks, 0, "{counters:?}");
}

#[test]
fn dense_bytecode_auto_executes_new_object_with_constructor() {
    let result = execute_source_with_options(
        r#"<?php
class DenseDto {
    public $id = 0;
    public $label = "";
    public int $typed = 1;
    private $secret = "s";
    protected $shade = "p";
    public function __construct($id, $label) {
        $this->id = $id;
        $this->label = $label;
        $this->typed = $id * 2;
    }
    public function summary() {
        return $this->id . ":" . $this->label . ":" . $this->typed
            . ":" . $this->secret . ":" . $this->shade;
    }
}
$total = 0;
$last = "";
for ($i = 1; $i <= 4; $i++) {
    $dto = new DenseDto($i, "row$i");
    $total += $dto->typed;
    $last = $dto->summary();
}
echo $total, "|", $last;
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"20|4:row4:8:s:p");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert_eq!(
        counters
            .dense_function_fallback_by_reason
            .get("object_instantiation"),
        None,
        "{counters:?}"
    );
    assert!(
        counters
            .opcodes
            .get("bytecode_new_object")
            .copied()
            .unwrap_or_default()
            >= 4,
        "{counters:?}"
    );
    assert!(counters.dense_activation_transfers >= 4, "{counters:?}");
    assert_eq!(counters.direct_call_owned_value_buffers, 0, "{counters:?}");
}

#[test]
fn dense_bytecode_auto_new_object_falls_back_for_magic_and_dynamic() {
    // Magic __set/__get and dynamic properties stay on the generic
    // helpers; output and diagnostics must match the rich interpreter.
    let source = r#"<?php
class DenseMagic {
    private $bag = [];
    public function __set($name, $value) {
        $this->bag[$name] = strtoupper($value);
    }
    public function __get($name) {
        return $this->bag[$name] ?? "absent";
    }
}
#[AllowDynamicProperties]
class DenseDyn {
    public $declared = "d";
}
$m = new DenseMagic();
$m->title = "quiet";
$d = new DenseDyn();
$d->extra = "x";
echo $m->title, "|", $m->missing, "|", $d->declared, "|", $d->extra;
"#;
    let auto = execute_source_with_options(
        source,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    let rich = execute_source_with_options(
        source,
        VmOptions {
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );
    assert!(auto.status.is_success(), "{:?}", auto.status);
    assert_eq!(auto.output.as_bytes(), b"QUIET|absent|d|x");
    assert_eq!(auto.output, rich.output);
    assert_eq!(auto.diagnostics, rich.diagnostics);
}

#[test]
fn dense_bytecode_new_object_reports_property_error_diagnostics() {
    // Typed property violations inside dense-instantiated objects keep
    // their catchable TypeError shape.
    let result = execute_source_with_options(
        r#"<?php
class DenseTyped {
    public int $n = 0;
}
$o = new DenseTyped();
try {
    $o->n = "nope";
} catch (TypeError $e) {
    echo "caught|", $o->n;
}
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"caught|0");
}

#[test]
fn dense_bytecode_auto_executes_isset_dim() {
    let result = execute_source_with_options(
        r#"<?php
$items = ['present' => 1, 'nullish' => null];
echo isset($items['present']) ? '1' : '0';
echo isset($items['nullish']) ? '1' : '0';
echo isset($items['missing']) ? '1' : '0';
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"100");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 1, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.dense_functions_executed >= 1, "{counters:?}");
    assert!(
        counters
            .dense_instruction_families_executed
            .get("arrays")
            .copied()
            .unwrap_or_default()
            >= 3,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_array_dim_reference_binding() {
    let result = execute_source_with_options(
        r#"<?php
function bind_array_dim_reference() {
    $items = [];
    $value = 7;
    $items['key'] =& $value;
    $value = 9;
    echo $items['key'], "\n";
    $items['key'] = 11;
    echo $value, "\n";
}
bind_array_dim_reference();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"9\n11\n");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 1, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert_eq!(
        counters.dense_bytecode_fallback_by_reference, 0,
        "{counters:?}"
    );
    assert!(counters.dense_functions_executed >= 2, "{counters:?}");
}

#[test]
fn dense_bytecode_auto_executes_instanceof() {
    let result = execute_source_with_options(
        r#"<?php
class InstanceProbe {}
class InstanceProbeChild extends InstanceProbe {}
function classify($value) {
    if ($value instanceof InstanceProbe) {
        return "probe";
    }
    if ($value instanceof Traversable) {
        return "traversable";
    }
    return "other";
}
echo classify(new InstanceProbeChild()), "\n";
echo classify(new InstanceProbe()), "\n";
echo classify(42), "\n";
echo classify(null), "\n";
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"probe\nprobe\nother\nother\n");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_instance_of")
            .copied()
            .unwrap_or_default()
            >= 4,
        "instanceof should execute densely: {counters:?}"
    );
    assert_eq!(
        counters
            .dense_function_fallback_by_reason
            .get("instruction_subset")
            .copied()
            .unwrap_or_default(),
        0,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_property_dim_probes() {
    let result = execute_source_with_options(
        r#"<?php
class ProbeHooks {
    public $callbacks = [];
}
$h = new ProbeHooks();
$h->callbacks[5]["cb"] = true;
function probe($h, $priority) {
    $state = isset($h->callbacks[$priority]) ? "set" : "unset";
    $state .= empty($h->callbacks[$priority]) ? ":empty" : ":filled";
    return $state;
}
echo probe($h, 5), "\n";
echo probe($h, 9), "\n";
echo probe(null, 1), "\n";
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.as_bytes(),
        b"set:filled\nunset:empty\nunset:empty\n"
    );
    let counters = result.counters.expect("counters should be collected");
    assert!(
        counters
            .opcodes
            .get("bytecode_isset_property_dim")
            .copied()
            .unwrap_or_default()
            >= 3,
        "{counters:?}"
    );
    assert!(
        counters
            .opcodes
            .get("bytecode_empty_property_dim")
            .copied()
            .unwrap_or_default()
            >= 3,
        "{counters:?}"
    );
    // The `$h->callbacks[5]["cb"] = true` setup (a property-dimension
    // assignment) now also executes on the dense bytecode path, so the entry no
    // longer deopts to the rich interpreter for it — the whole program runs
    // dense with no unsupported-opcode fallback.
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_assign_property_dim")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_fetch_const() {
    let result = execute_source_with_options(
        r#"<?php
define('DENSE_FETCH_CONST_PROBE', 'ok');
echo DENSE_FETCH_CONST_PROBE;
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"ok");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_lower_attempts, 1, "{counters:?}");
    assert_eq!(counters.bytecode_lower_successes, 1, "{counters:?}");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.dense_functions_executed >= 1, "{counters:?}");
    assert!(
        counters
            .dense_instruction_families_executed
            .get("constants")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn conditional_class_applies_property_defaults() {
    // Regression: a class declared conditionally (WordPress's ubiquitous
    // `if (!class_exists(...)) { class ... }`) must instantiate with its
    // property defaults applied, matching PHP. The semantics HIR lowering
    // previously skipped property-default expressions for class
    // declarations nested inside statements, so no const-expr id was
    // assigned, the IR carried `default: None`, and the properties read
    // null instead of their declared defaults.
    let result = execute_source(
        r#"<?php
if (!class_exists('CondPropDefault')) {
    class CondPropDefault {
        const C = 9;
        public $a = 'x';
        public $b = 1 + 2;
        public $d = self::C;
    }
}
$o = new CondPropDefault();
echo $o->a, '|', $o->b, '|', $o->d;
"#,
    );
    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "x|3|9");
}

#[test]
fn dense_bytecode_auto_executes_property_dim_assign() {
    // $obj->prop[$k] = $v (and [] append, nested dims, typed properties) now
    // execute on the dense bytecode path via the shared assign_property_dim_value
    // helper. Crucially, COW is preserved: a dimension write through a shared
    // array copy must leave the original array untouched.
    let result = execute_source_with_options(
        r#"<?php
class DenseBag { public $data = []; public array $typed = []; }
function dense_dim_probe() {
    $o = new DenseBag();
    $o->data['a'] = 1;
    $o->data['b'][] = 2;
    $o->typed['y']['z'] = 20;
    $shared = ['p' => 1];
    $o->data['shared'] = $shared;
    $o->data['shared']['p'] = 999;
    return json_encode($o->data) . '|' . $shared['p'];
}
echo dense_dim_probe();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        r#"{"a":1,"b":[2],"shared":{"p":999}}|1"#
    );
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert_eq!(counters.rich_fallback_functions_executed, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_assign_property_dim")
            .copied()
            .unwrap_or_default()
            >= 4,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_clone_object() {
    // `clone $obj` (including a __clone magic method) now executes on the dense
    // bytecode path via the shared clone_object_value helper.
    let result = execute_source_with_options(
        r#"<?php
class DenseClonePt {
    public $x = 1;
    public $y = 2;
    public $cloned = false;
    function __clone() { $this->cloned = true; }
}
function dense_clone_probe() {
    $a = new DenseClonePt();
    $a->x = 10;
    $b = clone $a;
    $b->x = 20;
    return $a->x . '|' . $b->x . '|' . $b->y . '|' . ($b->cloned ? '1' : '0');
}
echo dense_clone_probe();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "10|20|2|1");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_clone_object")
            .copied()
            .unwrap_or_default()
            >= 1,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_fetch_static_property() {
    // Class::$staticProperty fetches (public, typed, and private via self::)
    // now execute on the dense bytecode path via the shared
    // fetch_static_property_value helper.
    let result = execute_source_with_options(
        r#"<?php
class DenseStatic {
    public static $x = 5;
    public static string $name = 'hi';
    private static $secret = 99;
    static function reveal() { return self::$secret; }
}
function dense_static_probe() {
    return DenseStatic::$x . '|' . DenseStatic::$name . '|' . DenseStatic::reveal();
}
echo dense_static_probe();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "5|hi|99");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_fetch_static_property")
            .copied()
            .unwrap_or_default()
            >= 2,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_property_isset_empty() {
    // isset($obj->prop) / empty($obj->prop) (no dimensions) now execute on the
    // dense bytecode path via the shared isset_property_value/empty_property_value
    // helpers instead of deopting the enclosing function to the rich interpreter.
    let result = execute_source_with_options(
        r#"<?php
class DenseProbe { public $set = 1; public $nullp = null; }
function dense_probe_props() {
    $o = new DenseProbe();
    return (isset($o->set) ? '1' : '0')
        . (isset($o->nullp) ? '1' : '0')
        . (isset($o->missing) ? '1' : '0')
        . '|'
        . (empty($o->set) ? '1' : '0')
        . (empty($o->missing) ? '1' : '0');
}
echo dense_probe_props();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "100|01");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert_eq!(counters.rich_fallback_functions_executed, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_isset_property")
            .copied()
            .unwrap_or_default()
            >= 3,
        "{counters:?}"
    );
    assert!(
        counters
            .opcodes
            .get("bytecode_empty_property")
            .copied()
            .unwrap_or_default()
            >= 2,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_fetch_class_constant() {
    // Class-constant fetches (Class::CONST, inherited constants, self::-referencing
    // const-expr defaults, ::class, and enum cases) now execute on the dense
    // bytecode path via the shared fetch_class_constant_value helper instead of
    // deopting the whole function to the rich interpreter.
    let result = execute_source_with_options(
        r#"<?php
class DenseConstBase { const S = 7; }
class DenseConst extends DenseConstBase {
    const X = 42;
    const Y = self::X + 1;
    public const Z = 'z';
}
enum DenseSuit: string { case Hearts = 'H'; }
function dense_const_probe() {
    return DenseConst::X . '|' . DenseConst::Y . '|' . DenseConst::Z . '|' . DenseConst::S
        . '|' . DenseConst::class . '|' . DenseSuit::Hearts->value . '|' . DenseSuit::class;
}
echo dense_const_probe();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "42|43|z|7|DenseConst|H|DenseSuit"
    );
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert_eq!(counters.rich_fallback_functions_executed, 0, "{counters:?}");
    assert!(
        counters
            .opcodes
            .get("bytecode_fetch_class_constant")
            .copied()
            .unwrap_or_default()
            >= 5,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_auto_executes_conditional_declarations() {
    // WordPress-style conditional declarations (e.g.
    // `if (!function_exists(...)) { function ... }`, found throughout
    // pluggable.php, sodium_compat, and pomo/*) lower to runtime
    // DeclareFunction/DeclareClass instructions. These previously forced
    // the whole enclosing (usually include-entry) function onto the rich
    // interpreter; they now execute on the dense bytecode path.
    let result = execute_source_with_options(
        r#"<?php
if (!function_exists('dense_declared_probe')) {
    function dense_declared_probe() { return 'fn-ok'; }
}
if (!class_exists('DenseDeclaredProbe')) {
    class DenseDeclaredProbe {}
}
echo dense_declared_probe(), ':', class_exists('DenseDeclaredProbe') ? 'class-ok' : 'no';
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.as_bytes(), b"fn-ok:class-ok");
    let counters = result.counters.expect("counters should be collected");
    assert_eq!(counters.bytecode_unsupported_fallbacks, 0, "{counters:?}");
    assert!(counters.dense_functions_executed >= 1, "{counters:?}");
    assert!(
        counters
            .dense_instruction_families_executed
            .get("declarations")
            .copied()
            .unwrap_or_default()
            >= 2,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_executes_declared_property_fetch_and_assignment() {
    let result = execute_source_with_options(
        r#"<?php
class DensePropertyDto {
    public int $value = 1;
}
function dense_property_make_dto() {
    return new DensePropertyDto();
}
function dense_property_read($object) {
    return $object->value;
}
function dense_property_write($object, $value) {
    $object->value = $value;
    return $object->value;
}
$object = dense_property_make_dto();
for ($i = 0; $i < 4; $i++) {
    echo dense_property_read($object), ':', dense_property_write($object, $i), ';';
}
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "1:0;0:1;1:2;2:3;");
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.dense_property_fetch_hits >= 4, "{counters:?}");
    assert!(counters.dense_property_assignment_hits >= 4, "{counters:?}");
    assert!(counters.dense_property_ic_reuse > 0, "{counters:?}");
    assert!(
        counters
            .dense_instruction_families_executed
            .get("properties")
            .copied()
            .unwrap_or_default()
            >= 8,
        "{counters:?}"
    );
    assert!(
        !counters
            .dense_function_fallback_by_reason
            .contains_key("property_fetch"),
        "{counters:?}"
    );
    assert!(
        !counters
            .dense_function_fallback_by_reason
            .contains_key("property_assignment"),
        "{counters:?}"
    );
}

#[test]
fn dense_property_semantic_fallbacks_remain_local() {
    let result = execute_source_with_options(
        r#"<?php
class DensePropertyDynamic {
}
class DensePropertyMagic {
    public function __get($name) { return 'magic:' . $name; }
    public function __set($name, $value): void { echo 'set:' . $name . ':' . $value . ';'; }
}
class DensePropertyHook {
    public string $name { get { return 'hook'; } set { $this->name = strtoupper($value); } }
}
class DensePropertyTyped {
    public int $value = 0;
}
function dense_property_make_dynamic() {
    $object = new DensePropertyDynamic();
    $object->missing = 'dynamic';
    return $object;
}
function dense_property_make_magic() {
    return new DensePropertyMagic();
}
function dense_property_make_hook() {
    return new DensePropertyHook();
}
function dense_property_read_missing($object) {
    return $object->missing;
}
function dense_property_read_name($object) {
    return $object->name;
}
function dense_property_write_value($object, $value) {
    $object->value = $value;
    return $object->value;
}
$dynamic = dense_property_make_dynamic();
echo dense_property_read_missing($dynamic), ';';
$magic = dense_property_make_magic();
echo dense_property_read_missing($magic), ';';
dense_property_write_value($magic, 'x');
$hook = dense_property_make_hook();
echo dense_property_read_name($hook), ';';
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "dynamic;magic:missing;set:value:x;hook;"
    );
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.dense_functions_executed > 0, "{counters:?}");
    for reason in [
        "dynamic_property",
        "magic_get",
        "magic_set",
        "property_hook",
    ] {
        assert!(
            counters
                .dense_property_fallback_by_reason
                .contains_key(reason),
            "missing {reason}: {counters:?}"
        );
    }
    assert!(
        !counters
            .dense_function_fallback_by_reason
            .contains_key("property_fetch"),
        "{counters:?}"
    );
    assert!(
        !counters
            .dense_function_fallback_by_reason
            .contains_key("property_assignment"),
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_executes_method_and_static_calls() {
    let result = execute_source_with_options(
        r#"<?php
class DenseCallService {
    public function inc($value) { return $value + 1; }
    public static function twice($value) { return $value * 2; }
}
class DenseCallMagic {
    public function __call($name, $args) { return $name . count($args); }
}
function dense_call_make_service() {
    return new DenseCallService();
}
function dense_call_make_magic() {
    return new DenseCallMagic();
}
function dense_call_method($service, $value) {
    return $service->inc($value);
}
function dense_call_static($value) {
    return DenseCallService::twice($value);
}
function dense_call_magic($object) {
    return $object->missing(1, 2);
}
$service = dense_call_make_service();
for ($i = 0; $i < 4; $i++) {
    echo dense_call_method($service, $i), ':', dense_call_static($i), ';';
}
$magic = dense_call_make_magic();
echo dense_call_magic($magic);
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(result.output.to_string_lossy(), "1:0;2:2;3:4;4:6;missing2");
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.dense_method_call_hits >= 4, "{counters:?}");
    assert!(counters.dense_static_call_hits >= 4, "{counters:?}");
    assert!(counters.dense_call_ic_hits > 0, "{counters:?}");
    assert!(counters.dense_call_ic_misses > 0, "{counters:?}");
    assert!(
        counters
            .dense_call_fallback_by_reason
            .contains_key("magic_call"),
        "{counters:?}"
    );
    assert!(
        counters
            .dense_instruction_families_executed
            .get("function_calls")
            .copied()
            .unwrap_or_default()
            >= 8,
        "{counters:?}"
    );
}

#[test]
fn dense_bytecode_routes_pdo_runtime_methods() {
    let result = execute_source_with_options(
        r#"<?php
$db = new PDO("sqlite::memory:");
var_dump($db->exec("CREATE TABLE demo (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)"));
var_dump($db->prepare("INSERT INTO demo (name) VALUES (?)") instanceof PDOStatement);
for ($i = 0; $i < 3; $i++) {
    echo $db->getAttribute(PDO::ATTR_DRIVER_NAME), ':';
    var_dump($db->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION));
}
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(0)\nbool(true)\nsqlite:bool(true)\nsqlite:bool(true)\nsqlite:bool(true)\n"
    );
}

#[test]
fn dense_static_scoped_non_static_method_keeps_active_this() {
    let result = execute_source_with_options(
        r#"<?php
class DenseScopedBase {
    public function __construct() { echo 'base:' . $this->label() . ';'; }
    public function label() { return 'label'; }
}
class DenseScopedChild extends DenseScopedBase {
    public function __construct() {
        for ($i = 0; $i < 3; $i++) {
            parent::__construct();
            DenseScopedBase::label();
        }
    }
}
new DenseScopedChild();
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "base:label;base:label;base:label;"
    );
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.dense_static_call_hits >= 3, "{counters:?}");
    assert!(counters.dense_call_ic_hits > 0, "{counters:?}");

    let global = execute_source_with_options(
        "<?php class DenseScopedGlobal { public function label() { return 'x'; } } DenseScopedGlobal::label();",
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            inline_caches: InlineCacheMode::On,
            ..VmOptions::default()
        },
    );
    assert_eq!(global.status.exit_status(), ExitStatus::RuntimeError);
    assert!(
        global.diagnostics.iter().any(|diagnostic| diagnostic
            .message()
            .contains("densescopedglobal::label is not static")),
        "{:?}",
        global.diagnostics
    );
}

#[test]
fn dense_property_assignment_type_error_is_local_fallback() {
    let result = execute_source_with_options(
        r#"<?php
class DensePropertyTypedFailure {
    public int $value = 0;
}
function dense_property_make_typed_failure() {
    return new DensePropertyTypedFailure();
}
function dense_property_write_typed_failure($object, $value) {
    $object->value = $value;
    return $object->value;
}
$typed = dense_property_make_typed_failure();
dense_property_write_typed_failure($typed, 'bad');
"#,
        VmOptions {
            execution_format: ExecutionFormat::Auto,
            collect_counters: true,
            collect_profile_spans: false,
            collect_layout_source_attribution: true,
            ..VmOptions::default()
        },
    );

    assert_eq!(result.status.exit_status(), ExitStatus::RuntimeError);
    let counters = result.counters.expect("counters should be collected");
    assert!(counters.dense_functions_executed > 0, "{counters:?}");
    assert!(
        counters
            .dense_property_fallback_by_reason
            .contains_key("typed_property_validation"),
        "{counters:?}"
    );
    assert!(
        !counters
            .dense_function_fallback_by_reason
            .contains_key("property_assignment"),
        "{counters:?}"
    );
}

#[test]
fn set_time_limit_resets_or_disables_mutable_execution_deadline() {
    let reset = execute_source_with_options(
        "<?php set_time_limit(1); echo \"ok\\n\";",
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli("reset.php", Vec::new())
                .with_execution_time_limit(Some(Duration::ZERO)),
            max_steps: 1_000_000,
            ..VmOptions::default()
        },
    );
    assert!(reset.status.is_success(), "{:?}", reset.status);
    assert_eq!(reset.output.to_string_lossy(), "ok\n");

    let disabled = execute_source_with_options(
        "<?php set_time_limit(0); for ($i = 0; $i < 200; $i++) { } echo \"ok\\n\";",
        VmOptions {
            runtime_context: RuntimeContext::controlled_cli("disable.php", Vec::new())
                .with_execution_time_limit(Some(Duration::ZERO)),
            max_steps: 1_000_000,
            ..VmOptions::default()
        },
    );
    assert!(disabled.status.is_success(), "{:?}", disabled.status);
    assert_eq!(disabled.output.to_string_lossy(), "ok\n");
}

fn manual_return_unit(value: IrConstant) -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("manual.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 0),
    );
    let block = builder.append_block(function);
    let constant = builder.add_constant(value);
    builder.terminate_return(
        function,
        block,
        Some(Operand::Constant(constant)),
        IrSpan::new(file, 0, 0),
    );
    builder.set_entry(function);
    builder.finish()
}

fn execute_source(source: &str) -> VmResult {
    execute_source_with_options(source, VmOptions::default())
}

fn normalize_object_debug_ids(output: &str) -> String {
    let mut normalized = String::with_capacity(output.len());
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        normalized.push(ch);
        if ch == '#' && chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
                let _ = chars.next();
            }
            normalized.push_str("%d");
        }
    }
    normalized
}

fn first_vm_compile_payload(result: &VmResult) -> &VmCompileDiagnostic {
    result
        .diagnostics
        .iter()
        .find_map(|diagnostic| match diagnostic.payload()? {
            RuntimeDiagnosticPayload::VmCompile(payload) => Some(payload),
            RuntimeDiagnosticPayload::JsonBuiltin(_) => None,
            RuntimeDiagnosticPayload::TokenizerParse(_) => None,
            RuntimeDiagnosticPayload::Bringup(_) => None,
            RuntimeDiagnosticPayload::IncludeFailure(_) => None,
        })
        .expect("compile error should carry VM compile payload")
}

fn first_runtime_bringup_payload(
    result: &VmResult,
) -> &php_runtime::api::RuntimeBringupDiagnosticContext {
    result
        .diagnostics
        .iter()
        .find_map(|diagnostic| match diagnostic.payload()? {
            RuntimeDiagnosticPayload::Bringup(payload) => Some(payload),
            RuntimeDiagnosticPayload::JsonBuiltin(_) => None,
            RuntimeDiagnosticPayload::TokenizerParse(_) => None,
            RuntimeDiagnosticPayload::VmCompile(_) => None,
            RuntimeDiagnosticPayload::IncludeFailure(_) => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "diagnostic should carry runtime bring-up payload: {:#?}",
                result.diagnostics
            )
        })
}

fn execute_source_with_options(source: &str, options: VmOptions) -> VmResult {
    execute_source_with_options_and_path(source, options, "/tmp/phrust-test.php".to_owned())
}

fn execute_source_with_options_and_path(
    source: &str,
    options: VmOptions,
    source_path: String,
) -> VmResult {
    let frontend = php_semantics::analyze_source(source);
    assert!(
        !frontend.has_errors(),
        "frontend errors: {:?}",
        frontend.semantic_diagnostics()
    );
    let lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_path,
            source_text: Some(source.to_owned()),
            ..php_ir::LoweringOptions::default()
        },
    );
    assert!(
        lowering.diagnostics.is_empty(),
        "{:#?}",
        lowering.diagnostics
    );
    assert!(
        lowering.verification.is_ok(),
        "{:#?}",
        lowering.verification
    );
    Vm::with_options(options).execute(lowering.unit)
}

fn execute_temp_source_file(name: &str, source: &str) -> VmResult {
    let path = std::env::temp_dir().join(format!("phrust-vm-{}-{name}.php", std::process::id()));
    std::fs::write(&path, source).expect("temporary PHP source should be writable");
    let result = execute_source_with_options_and_path(
        source,
        VmOptions::default(),
        path.to_string_lossy().into_owned(),
    );
    let _ = std::fs::remove_file(path);
    result
}

fn execute_fixture_file(path: &str) -> VmResult {
    execute_fixture_file_with_options(path, VmOptions::default())
}

fn execute_fixture_file_with_options(path: &str, options: VmOptions) -> VmResult {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate should live under workspace/crates/php_vm");
    let path = workspace.join(path);
    let source = std::fs::read_to_string(&path).expect("fixture should be readable");
    let frontend = php_semantics::analyze_source(&source);
    assert!(
        !frontend.has_errors(),
        "frontend errors: {:?}",
        frontend.semantic_diagnostics()
    );
    let canonical = std::fs::canonicalize(&path).expect("fixture should canonicalize");
    let lowering = php_ir::lower_frontend_result(
        &frontend,
        php_ir::LoweringOptions {
            source_path: canonical.to_string_lossy().into_owned(),
            source_text: Some(source),
            ..php_ir::LoweringOptions::default()
        },
    );
    assert!(
        lowering.diagnostics.is_empty(),
        "{:#?}",
        lowering.diagnostics
    );
    assert!(
        lowering.verification.is_ok(),
        "{:#?}",
        lowering.verification
    );
    let loader = IncludeLoader::for_root(
        canonical
            .parent()
            .expect("fixture should have parent")
            .to_path_buf(),
    )
    .expect("include loader should initialize");
    Vm::with_options(VmOptions {
        include_loader: Some(loader),
        ..options
    })
    .execute(lowering.unit)
}

fn assert_uncaught_exception_output_prefix(
    output: &str,
    prefix: &str,
    class_name: &str,
    message: &str,
) {
    assert!(output.starts_with(prefix), "{output}");
    assert!(
        output.contains(&format!(
            "\nFatal error: Uncaught {class_name}: {message} in "
        )),
        "{output}"
    );
    assert!(output.contains("Stack trace:\n#0 "), "{output}");
    assert!(output.contains("  thrown in "), "{output}");
}

fn runtime_trace_events(trace: &[String]) -> Vec<String> {
    trace
        .iter()
        .filter_map(|line| {
            line.split_once(" runtime ")
                .map(|(_, event)| event.to_owned())
        })
        .collect()
}

fn include_trace_events(trace: &[String]) -> Vec<String> {
    trace
        .iter()
        .filter_map(|line| {
            line.split_once(" include ")
                .map(|(_, event)| event.to_owned())
        })
        .collect()
}

fn assert_trace_is_normalized(trace: &[String]) {
    assert!(
        trace.iter().all(|line| {
            !line.contains("0x")
                && !line.contains(" at ")
                && !line.contains("id:")
                && !line.contains("id=")
        }),
        "{trace:#?}"
    );
}

fn manual_echo_unit(value: IrConstant) -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("manual.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 0),
    );
    let block = builder.append_block(function);
    let value = builder.add_constant(value);
    let null = builder.add_constant(IrConstant::Null);
    let register = builder.alloc_register(function);
    builder.emit_load_const(function, block, register, value, IrSpan::new(file, 0, 0));
    builder.emit(
        function,
        block,
        InstructionKind::Echo {
            src: Operand::Register(register),
        },
        IrSpan::new(file, 0, 0),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Constant(null)),
        IrSpan::new(file, 0, 0),
    );
    builder.set_entry(function);
    builder.finish()
}

fn manual_unsupported_unit() -> php_ir::IrUnit {
    manual_unsupported_unit_for("E_PHP_RUNTIME_UNSUPPORTED_GENERATOR_EXECUTION")
}

fn manual_unsupported_unit_for(diagnostic_id: &str) -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("manual.php");
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        IrSpan::new(file, 0, 0),
    );
    let block = builder.append_block(function);
    let null = builder.add_constant(IrConstant::Null);
    builder.emit(
        function,
        block,
        InstructionKind::Unsupported {
            diagnostic_id: diagnostic_id.to_owned(),
        },
        IrSpan::new(file, 0, 0),
    );
    builder.terminate_return(
        function,
        block,
        Some(Operand::Constant(null)),
        IrSpan::new(file, 0, 0),
    );
    builder.set_entry(function);
    builder.finish()
}

// ---------------------------------------------------------------------------
// Runtime lever R3: last-use move correctness fixtures.
//
// Each fixture asserts identical PHP-visible output under three configurations:
// the rich-IR interpreter (independent oracle), dense bytecode with
// `last_use_moves` OFF (the default clone path), and dense bytecode with
// `last_use_moves` ON. All three must equal the pinned PHP 8.5.7 output.
// ---------------------------------------------------------------------------

fn run_last_use_config(source: &str, format: ExecutionFormat, last_use_moves: bool) -> VmResult {
    execute_source_with_options(
        source,
        VmOptions {
            execution_format: format,
            last_use_moves,
            collect_counters: true,
            ..VmOptions::default()
        },
    )
}

#[track_caller]
fn assert_last_use_move_parity(source: &str, expected: &str) {
    let reference = run_last_use_config(source, ExecutionFormat::Ir, false);
    assert!(
        reference.status.is_success(),
        "rich-IR reference failed: {:?}",
        reference.status
    );
    assert_eq!(
        reference.output.to_string_lossy(),
        expected,
        "rich-IR reference output mismatch"
    );

    let flag_off = run_last_use_config(source, ExecutionFormat::Auto, false);
    assert!(
        flag_off.status.is_success(),
        "dense flag-off failed: {:?}",
        flag_off.status
    );
    assert_eq!(
        flag_off.output.to_string_lossy(),
        expected,
        "dense last-use-moves=off output mismatch"
    );

    let flag_on = run_last_use_config(source, ExecutionFormat::Auto, true);
    assert!(
        flag_on.status.is_success(),
        "dense flag-on failed: {:?}",
        flag_on.status
    );
    assert_eq!(
        flag_on.output.to_string_lossy(),
        expected,
        "dense last-use-moves=on output mismatch"
    );

    // The move optimization must never change PHP-visible output.
    assert_eq!(
        flag_on.output.to_string_lossy(),
        flag_off.output.to_string_lossy(),
        "last-use-moves changed observable output"
    );
}

#[test]
fn last_use_move_preserves_array_copy_on_write() {
    assert_last_use_move_parity(
        "<?php\n$b = [1, 2, 3];\n$a = $b;\n$b[] = 4;\necho count($a), \"|\", count($b), \"\\n\";\necho implode(\",\", $a), \"|\", implode(\",\", $b), \"\\n\";\n",
        "3|4\n1,2,3|1,2,3,4\n",
    );
}

#[test]
fn last_use_move_preserves_foreach_by_value_and_by_reference() {
    assert_last_use_move_parity(
        "<?php\n$arr = [1, 2, 3];\nforeach ($arr as $v) { $v = $v * 10; }\necho implode(\",\", $arr), \"\\n\";\nforeach ($arr as &$r) { $r = $r * 10; }\nunset($r);\necho implode(\",\", $arr), \"\\n\";\n",
        "1,2,3\n10,20,30\n",
    );
}

#[test]
fn last_use_move_preserves_reference_identity() {
    assert_last_use_move_parity(
        "<?php\n$x = 1;\n$r = &$x;\n$r = 5;\necho $x, \"\\n\";\n$y = [1, 2];\n$ry = &$y;\n$ry[] = 3;\necho implode(\",\", $y), \"\\n\";\n",
        "5\n1,2,3\n",
    );
}

#[test]
fn last_use_move_preserves_nested_dim_writes() {
    assert_last_use_move_parity(
        "<?php\n$m = [];\n$m[\"a\"][\"b\"] = 1;\n$m[\"a\"][\"c\"] = 2;\n$n = $m;\n$n[\"a\"][\"b\"] = 99;\necho $m[\"a\"][\"b\"], \"|\", $m[\"a\"][\"c\"], \"\\n\";\necho $n[\"a\"][\"b\"], \"|\", $n[\"a\"][\"c\"], \"\\n\";\n",
        "1|2\n99|2\n",
    );
}

#[test]
fn last_use_move_does_not_corrupt_value_read_twice() {
    assert_last_use_move_parity(
        "<?php\n$s = \"abc\";\n$t = $s . $s;\necho $t, \"\\n\";\necho $s, \"\\n\";\n$a = [1, 2];\n$b = $a + $a;\necho count($b), \"\\n\";\necho implode(\",\", $a), \"\\n\";\n",
        "abcabc\nabc\n2\n1,2\n",
    );
}

#[test]
fn last_use_move_preserves_array_passed_by_value() {
    assert_last_use_move_parity(
        "<?php\nfunction sum_arr(array $a): int {\n    $t = 0;\n    foreach ($a as $v) { $t += $v; }\n    return $t;\n}\n$data = [1, 2, 3];\n$s = sum_arr($data);\n$data[] = 4;\necho $s, \"\\n\";\necho implode(\",\", $data), \"\\n\";\n",
        "6\n1,2,3,4\n",
    );
}

#[test]
fn last_use_move_preserves_string_copy_on_write() {
    assert_last_use_move_parity(
        "<?php\n$s = \"hello\";\n$t = $s;\n$t .= \" world\";\necho $s, \"\\n\";\necho $t, \"\\n\";\n",
        "hello\nhello world\n",
    );
}

#[test]
fn last_use_move_fires_on_dense_cast_of_register_value() {
    // The `$a . $b` concat lands a heap string in a register whose sole,
    // block-local last use is the `(int)` cast source: the flag-on run must move
    // it (a real string clone avoided) while producing identical output.
    let source = "<?php\n$a = \"12\";\n$b = \"34\";\n$n = (int)($a . $b);\necho $n, \"\\n\";\n";

    let flag_off = run_last_use_config(source, ExecutionFormat::Bytecode, false);
    assert!(
        flag_off.status.is_success(),
        "flag-off failed: {:?}",
        flag_off.status
    );
    assert_eq!(flag_off.output.to_string_lossy(), "1234\n");
    let off_counters = flag_off.counters.expect("counters enabled");
    assert_eq!(off_counters.last_use_moves_applied, 0);

    let flag_on = run_last_use_config(source, ExecutionFormat::Bytecode, true);
    assert!(
        flag_on.status.is_success(),
        "flag-on failed: {:?}",
        flag_on.status
    );
    assert_eq!(flag_on.output.to_string_lossy(), "1234\n");
    let on_counters = flag_on.counters.expect("counters enabled");
    assert!(
        on_counters.last_use_moves_applied >= 1,
        "expected at least one applied last-use move, got {}",
        on_counters.last_use_moves_applied
    );
    assert!(
        on_counters.last_use_move_clones_avoided >= 1,
        "expected a heap clone to be avoided, got {}",
        on_counters.last_use_move_clones_avoided
    );
}

#[test]
fn last_use_array_read_release_preserves_reference_and_cow_semantics() {
    // Array-read register release must not perturb PHP-visible semantics: an
    // aliased element reference keeps aliasing, and a by-value array copy stays
    // independent, across intervening in-place writes.
    assert_last_use_move_parity(
        "<?php\n$map = [\"a\"=>1, \"b\"=>2, \"c\"=>3];\n$r = &$map[\"b\"];\n$read = $map[\"a\"];\n$map[\"c\"] = 100;\n$r = 555;\n$copy = $map;\n$peek = $map[\"a\"];\n$map[\"c\"] = 7;\necho $map[\"b\"], \"|\", $map[\"c\"], \"|\", $read, \"\\n\";\necho implode(\",\", $copy), \"\\n\";\necho $peek, \"\\n\";\n",
        "555|7|1\n1,555,100\n1\n",
    );
}

#[test]
fn last_use_array_read_release_eliminates_false_sharing_cow() {
    // Read-then-write in a loop: `$map["b"]` loads $map into a register whose
    // block-local last use is the fetch. Flag-off that clone lingers across
    // `$map["c"] = $i`, so the local looks shared and every iteration COW-
    // separates. Flag-on releases the dead handle before the write, which then
    // mutates in place: `cow_separations` collapses while output is unchanged.
    let source = "<?php\n$map = [\"a\"=>1,\"b\"=>2,\"c\"=>3];\n$sum = 0;\nfor ($i = 0; $i < 50; $i++) {\n    $sum += $map[\"b\"];\n    $map[\"c\"] = $i;\n}\necho $sum, \"|\", $map[\"c\"], \"\\n\";\n";

    let flag_off = run_last_use_config(source, ExecutionFormat::Bytecode, false);
    assert!(
        flag_off.status.is_success(),
        "flag-off failed: {:?}",
        flag_off.status
    );
    assert_eq!(flag_off.output.to_string_lossy(), "100|49\n");
    let off_counters = flag_off.counters.expect("counters enabled");
    assert_eq!(off_counters.last_use_array_read_releases, 0);
    assert!(
        off_counters.cow_separations >= 50,
        "flag-off must copy-on-write every iteration, got {}",
        off_counters.cow_separations
    );

    let flag_on = run_last_use_config(source, ExecutionFormat::Bytecode, true);
    assert!(
        flag_on.status.is_success(),
        "flag-on failed: {:?}",
        flag_on.status
    );
    assert_eq!(
        flag_on.output.to_string_lossy(),
        "100|49\n",
        "array-read release must not change observable output"
    );
    let on_counters = flag_on.counters.expect("counters enabled");
    assert!(
        on_counters.last_use_array_read_releases >= 50,
        "expected a per-iteration array-read release, got {}",
        on_counters.last_use_array_read_releases
    );
    assert!(
        on_counters.cow_separations < off_counters.cow_separations,
        "release must cut copy-on-write separations: off={} on={}",
        off_counters.cow_separations,
        on_counters.cow_separations
    );
}

// ---------------------------------------------------------------------------
// Runtime lever R4: class-context frame-reuse correctness fixtures.
//
// Every fixture asserts byte-identical PHP-visible output with
// `reuse_class_context_frames` OFF (the default fresh-frame path, where every
// class-context call is blocked with reason `class_context`) and ON (the
// class-context pooling path). Method/constructor/static calls that clear every
// other reuse guard must reuse a pooled frame without changing `$this` identity,
// destructor order, reference identity, static-local state, recursion results,
// or exception/finally unwinding. Parity is checked on both the rich-IR path and
// the dense-bytecode path (both call sites thread the flag).
// ---------------------------------------------------------------------------

fn run_reuse_class_context_config(
    source: &str,
    format: ExecutionFormat,
    reuse_class_context_frames: bool,
) -> VmResult {
    execute_source_with_options(
        source,
        VmOptions {
            execution_format: format,
            reuse_class_context_frames,
            collect_counters: true,
            ..VmOptions::default()
        },
    )
}

#[track_caller]
fn assert_reuse_class_context_parity(source: &str, expected: &str) {
    // Rich-IR path pins the expected bytes and proves the flag is a no-op there.
    let ir_off = run_reuse_class_context_config(source, ExecutionFormat::Ir, false);
    assert!(
        ir_off.status.is_success(),
        "rich-IR flag-off failed: {:?}",
        ir_off.status
    );
    assert_eq!(
        ir_off.output.to_string_lossy(),
        expected,
        "rich-IR flag-off output mismatch"
    );
    let ir_on = run_reuse_class_context_config(source, ExecutionFormat::Ir, true);
    assert!(
        ir_on.status.is_success(),
        "rich-IR flag-on failed: {:?}",
        ir_on.status
    );
    assert_eq!(
        ir_on.output.to_string_lossy(),
        ir_off.output.to_string_lossy(),
        "rich-IR: reuse-class-context-frames changed observable output"
    );

    // Dense-bytecode path: the flag must not change observable output either.
    let dense_off = run_reuse_class_context_config(source, ExecutionFormat::Auto, false);
    assert!(
        dense_off.status.is_success(),
        "dense flag-off failed: {:?}",
        dense_off.status
    );
    let dense_on = run_reuse_class_context_config(source, ExecutionFormat::Auto, true);
    assert!(
        dense_on.status.is_success(),
        "dense flag-on failed: {:?}",
        dense_on.status
    );
    assert_eq!(
        dense_on.output.to_string_lossy(),
        dense_off.output.to_string_lossy(),
        "dense: reuse-class-context-frames changed observable output"
    );
}

#[test]
fn reuse_class_context_preserves_recursive_method_results() {
    // Recursive method (`$this->fact`) invoked in a loop: each recursion level
    // gets a distinct frame while parents are live, and the sibling iterations
    // reuse recycled frames. A wrong result would prove a reused frame aliased a
    // still-live parent.
    assert_reuse_class_context_parity(
        "<?php\nclass Math {\n    function fact(int $n): int {\n        if ($n <= 1) return 1;\n        return $n * $this->fact($n - 1);\n    }\n}\n$m = new Math();\n$total = 0;\nfor ($i = 1; $i <= 8; $i++) { $total += $m->fact($i); }\necho $total, \"\\n\";\necho $m->fact(12), \"\\n\";\n",
        "46233\n479001600\n",
    );
}

#[test]
fn reuse_class_context_does_not_leak_stale_this() {
    // A method stores `$this` in a static local, then a later call from a
    // different object must observe the new `$this`, never the pooled prior
    // occupant's. Also exercises static-local persistence across reused frames.
    assert_reuse_class_context_parity(
        "<?php\nclass Box {\n    public int $id;\n    function __construct(int $id) { $this->id = $id; }\n    function remember(): void {\n        static $last = null;\n        if ($last !== null) { echo \"prev={$last->id} cur={$this->id}\\n\"; }\n        else { echo \"first cur={$this->id}\\n\"; }\n        $last = $this;\n    }\n}\n$a = new Box(1);\n$b = new Box(2);\n$c = new Box(3);\n$a->remember();\n$b->remember();\n$c->remember();\n$a->remember();\n",
        "first cur=1\nprev=1 cur=2\nprev=2 cur=3\nprev=3 cur=1\n",
    );
}

#[test]
fn reuse_class_context_preserves_destructor_order() {
    // Objects with `__destruct` created and freed across method calls: the
    // object-creating function stays fresh (destructor-sensitive body), while the
    // `label()` method it calls is reuse-eligible. Destructor print order must be
    // identical flag-off and flag-on.
    assert_reuse_class_context_parity(
        "<?php\nclass Node {\n    public int $id;\n    function __construct(int $id) { $this->id = $id; echo \"make {$id}\\n\"; }\n    function __destruct() { echo \"destroy {$this->id}\\n\"; }\n    function label(): string { return \"node{$this->id}\"; }\n}\nfunction process(int $id): string {\n    $n = new Node($id);\n    return $n->label();\n}\nfor ($i = 1; $i <= 4; $i++) {\n    echo process($i), \"\\n\";\n}\necho \"done\\n\";\n",
        "make 1\ndestroy 1\nnode1\nmake 2\ndestroy 2\nnode2\nmake 3\ndestroy 3\nnode3\nmake 4\ndestroy 4\nnode4\ndone\n",
    );
}

#[test]
fn reuse_class_context_preserves_this_property_and_reference_identity() {
    // Property mutation through `$this` across reused-frame calls, plus a
    // reference bound to a `$this` property inside a reused method body.
    assert_reuse_class_context_parity(
        "<?php\nclass Acc {\n    public int $total = 0;\n    function add(int $v): void { $this->total += $v; }\n    function get(): int { return $this->total; }\n}\n$acc = new Acc();\nfor ($i = 1; $i <= 10; $i++) { $acc->add($i); }\necho $acc->get(), \"\\n\";\n\nclass RefBox {\n    public int $v = 0;\n    function bump(): void { $r = &$this->v; $r++; }\n}\n$b = new RefBox();\n$b->bump(); $b->bump(); $b->bump();\necho $b->v, \"\\n\";\n",
        "55\n3\n",
    );
}

#[test]
fn reuse_class_context_preserves_exception_and_finally_unwinding() {
    // Methods with try/finally stay fresh (try_finally guard), but sibling
    // reuse-eligible methods interleave. Exception propagation and finally order
    // must be identical flag-off and flag-on.
    assert_reuse_class_context_parity(
        "<?php\nclass Worker {\n    function risky(int $n): int {\n        try {\n            if ($n % 2 === 0) { throw new RuntimeException(\"even {$n}\"); }\n            return $n * 10;\n        } finally {\n            echo \"finally {$n}\\n\";\n        }\n    }\n    function safe(int $n): int { return $n + 1; }\n}\n$w = new Worker();\nfor ($i = 1; $i <= 4; $i++) {\n    try {\n        echo \"got \", $w->risky($i), \"\\n\";\n    } catch (RuntimeException $e) {\n        echo \"caught \", $e->getMessage(), \"\\n\";\n    }\n    echo \"safe \", $w->safe($i), \"\\n\";\n}\n",
        "got finally 1\n10\nsafe 2\ngot finally 2\ncaught even 2\nsafe 3\ngot finally 3\n30\nsafe 4\ngot finally 4\ncaught even 4\nsafe 5\n",
    );
}

#[test]
fn reuse_class_context_preserves_static_local_persistence() {
    // A per-method static counter must keep incrementing across reused frames.
    assert_reuse_class_context_parity(
        "<?php\nclass Seq {\n    function next(): int {\n        static $n = 0;\n        $n++;\n        return $n;\n    }\n}\n$s = new Seq();\n$out = \"\";\nfor ($i = 0; $i < 6; $i++) { $out .= $s->next(); }\necho $out, \"\\n\";\n",
        "123456\n",
    );
}

#[test]
fn reuse_class_context_preserves_deep_oop_call_chain() {
    // A four-deep method chain driven in a loop: recycled frames from one
    // iteration feed the next without corrupting the running sum.
    assert_reuse_class_context_parity(
        "<?php\nclass Chain {\n    function a(int $x): int { return $this->b($x + 1); }\n    function b(int $x): int { return $this->c($x + 1); }\n    function c(int $x): int { return $this->d($x + 1); }\n    function d(int $x): int { return $x + 1; }\n}\n$ch = new Chain();\n$sum = 0;\nfor ($i = 0; $i < 100; $i++) { $sum += $ch->a($i); }\necho $sum, \"\\n\";\n",
        "5350\n",
    );
}

#[test]
fn reuse_class_context_signal_reuses_frames_and_clears_blocker() {
    // Signal proof: on a method-call loop the default path reuses nothing and
    // reports `class_context` for every call; with the flag on, frames are reused,
    // allocations drop, and `class_context` no longer appears — output unchanged.
    let source = "<?php\nclass Calc {\n    function add(int $a, int $b): int { return $a + $b; }\n}\n$c = new Calc();\n$sum = 0;\nfor ($i = 0; $i < 50; $i++) { $sum += $c->add($i, $i); }\necho $sum, \"\\n\";\n";

    let flag_off = run_reuse_class_context_config(source, ExecutionFormat::Ir, false);
    assert!(
        flag_off.status.is_success(),
        "flag-off failed: {:?}",
        flag_off.status
    );
    assert_eq!(flag_off.output.to_string_lossy(), "2450\n");
    let off = flag_off.counters.expect("counters enabled");
    assert_eq!(
        off.frames_reused, 0,
        "flag-off must reuse no frames for method calls"
    );
    assert!(
        off.frame_reuse_blocked_by_reason
            .get("class_context")
            .copied()
            .unwrap_or(0)
            >= 50,
        "flag-off must block every method call on class_context, got {:?}",
        off.frame_reuse_blocked_by_reason.get("class_context")
    );

    let flag_on = run_reuse_class_context_config(source, ExecutionFormat::Ir, true);
    assert!(
        flag_on.status.is_success(),
        "flag-on failed: {:?}",
        flag_on.status
    );
    assert_eq!(
        flag_on.output.to_string_lossy(),
        flag_off.output.to_string_lossy(),
        "reuse-class-context-frames changed observable output"
    );
    let on = flag_on.counters.expect("counters enabled");
    assert!(
        on.frames_reused >= 1,
        "flag-on must reuse class-context frames, got {}",
        on.frames_reused
    );
    assert_eq!(
        on.frames_reused, on.register_files_reused,
        "frame and register-file reuse must track together"
    );
    assert!(
        !on.frame_reuse_blocked_by_reason
            .contains_key("class_context"),
        "flag-on must not report class_context, got {:?}",
        on.frame_reuse_blocked_by_reason
    );
    assert!(
        on.frames_allocated < off.frames_allocated,
        "flag-on must allocate fewer frames: off={} on={}",
        off.frames_allocated,
        on.frames_allocated
    );
}

#[test]
fn resolved_constant_reads_stay_cow_isolated() {
    // Repeated reads of one string/array constant share the cached
    // resolved value; a mutation through any handle must separate from
    // the cached storage instead of corrupting later reads.
    let result = execute_source(
        "<?php
for ($i = 0; $i < 3; $i++) {
    $a = ['x' => 1, 'y' => 'base'];
    $a['x'] = $a['x'] + $i;
    $a['y'] .= '-mutated';
    echo $a['x'], ':', $a['y'], \"\\n\";
}
for ($i = 0; $i < 2; $i++) {
    $s = 'abc';
    $s[0] = 'X';
    echo $s, \"\\n\";
}",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "1:base-mutated\n2:base-mutated\n3:base-mutated\nXbc\nXbc\n"
    );
}

#[test]
fn magic_get_class_getters_stay_reference_exact_across_unset() {
    // Classes with a hierarchy __get (every WordPress core class) now admit
    // property-load leaves for their declared accessible slots. A declared
    // property never routes through __get while set; unset() empties its
    // runtime storage, which the native helper answers with a storage side
    // exit, so the interpreter re-arms __get. The getter runs enough
    // iterations for the native leaf to engage BEFORE the unset, so the
    // post-unset calls prove the side exit, not just cold interpretation.
    let result = execute_source(
        "<?php
class Compat {
    public $flag = true;
    public function __get($name) { return \"magic:$name\"; }
    public function flag() { return (bool) $this->flag; }
    public function raw() { return $this->flag; }
}
$c = new Compat();
for ($i = 0; $i < 3; $i++) {
    var_dump($c->flag(), $c->raw());
}
unset($c->flag);
for ($i = 0; $i < 2; $i++) {
    var_dump($c->raw());
}",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        format!(
            "{}{}",
            "bool(true)\nbool(true)\n".repeat(3),
            "string(10) \"magic:flag\"\n".repeat(2)
        ),
        "unset must re-arm __get identically on native-cached getters"
    );
}

#[test]
fn magic_set_class_setters_stay_reference_exact_across_unset() {
    // The write-side mirror: a declared untyped slot admits the store leaf
    // despite a hierarchy __set; unset() empties the storage and the store
    // helper side-exits before writing, so the interpreter invokes __set.
    let result = execute_source(
        "<?php
class Compat {
    public $slot = 0;
    public $seen = [];
    public function __set($name, $value) { $this->seen[] = \"$name=$value\"; }
    public function put($v) { $this->slot = $v; }
}
$c = new Compat();
for ($i = 0; $i < 3; $i++) {
    $c->put($i);
}
var_dump($c->slot);
unset($c->slot);
$c->put(9);
$c->put(11);
var_dump(isset($c->slot), $c->seen);",
    );

    assert!(result.status.is_success(), "{:?}", result.status);
    assert_eq!(
        result.output.to_string_lossy(),
        "int(2)\nbool(false)\narray(2) {\n  [0]=>\n  string(6) \"slot=9\"\n  [1]=>\n  string(7) \"slot=11\"\n}\n",
        "unset must re-arm __set identically on native-cached setters"
    );
}
