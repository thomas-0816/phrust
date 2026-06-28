//! Transport-independent PHP execution facade.
//!
//! `php_executor` is the canonical in-process compile/execute owner for the VM
//! CLI compatibility path and the integrated HTTP server. It owns source
//! analysis, IR lowering, optimization, VM invocation, request include-loader
//! construction, PHP diagnostic rendering, and the process-local compiled-script
//! cache used by the server.
//!
//! The crate intentionally does not own HTTP routing, CLI argument parsing, disk
//! bytecode artifact caching, or debug/report commands that need direct access
//! to frontend or VM internals.

mod cache;
mod diagnostics;
mod engine_compat;
mod executor;
mod input;
mod pipeline;
mod request;

pub use cache::{
    CompiledScriptCache, CompiledScriptCacheLookup, CompiledScriptCacheStats, PhpScriptCacheInput,
};
pub use engine_compat::{CliIniOptions, EngineInput, execute_php, read_script};
pub use executor::{CompiledPhpScript, PhpExecutor};
pub use input::{
    PhpCompileInput, PhpExecutionError, PhpExecutionInput, PhpExecutionOutput, PhpExecutionStatus,
    PhpExecutorOptions, PhpRequestExecutionInput,
};
pub use php_optimizer::OptimizationLevel;

#[cfg(test)]
mod tests {
    use crate::diagnostics::{line_number_for_span, write_php_fatal_line};
    use crate::engine_compat::EXIT_PHP_ERROR;
    use php_runtime::api::RuntimeContext;
    use php_source::{SourceText, TextRange};
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn line_number_for_span_uses_one_based_source_lines() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        assert_eq!(line_number_for_span(&source, TextRange::new(6, 14)), 2);
    }

    #[test]
    fn php_fatal_line_matches_php_compile_error_shape() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        let mut stderr = Vec::new();

        write_php_fatal_line(
            &mut stderr,
            "fixture.php",
            &source,
            TextRange::new(6, 14),
            "Type callable cannot be part of an intersection type",
        )
        .expect("fatal line should render");

        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Type callable cannot be part of an intersection type in fixture.php on line 2\n"
        );
    }

    #[test]
    fn php_executor_executes_source() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: "<?php echo \"hello\\n\";".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("fixture.php", Vec::new()),
            optimization_level: None,
            collect_counters: false,
        });

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"hello\n");
        assert!(output.diagnostics_text.is_empty());
        assert!(output.runtime_diagnostics.is_empty());
        assert!(output.counters.is_none());
    }

    #[test]
    fn php_executor_reports_compile_errors() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: "<?php function {".to_owned(),
            source_path: "broken.php".to_owned(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("broken.php", Vec::new()),
            optimization_level: None,
            collect_counters: false,
        });

        assert_eq!(output.status, PhpExecutionStatus::CompileError);
        assert!(output.stdout.is_empty());
        assert!(
            output.diagnostics_text.contains("Parse error")
                || output.diagnostics_text.contains("syntax error")
                || output.diagnostics_text.contains("expected_identifier"),
            "{}",
            output.diagnostics_text
        );
    }

    #[test]
    fn php_executor_executes_compiled_script_with_http_context() {
        let executor = PhpExecutor::new();
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php echo $_SERVER['REQUEST_METHOD'], '|', $_GET['name'];".to_owned(),
                source_path: "public/index.php".to_owned(),
                optimization_level: None,
            })
            .expect("compile source");
        let request = php_runtime::RuntimeHttpRequestContext::new(
            "GET",
            "localhost",
            "/index.php?name=phrust",
            "/index.php",
            "/srv/public/index.php",
            "/srv/public",
        );

        let output = executor.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: Some(PathBuf::from("/srv/public/index.php")),
                cwd: PathBuf::from("/srv/public"),
                include_roots: vec![PathBuf::from(".")],
                runtime_context: RuntimeContext::controlled_http(request),
                collect_counters: false,
            },
        );

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"GET|phrust");
        assert!(output.diagnostics_text.is_empty());
    }

    #[test]
    fn compiled_script_cache_hits_after_first_compile() {
        let fixture = CacheFixture::new("cache-hit");
        fixture.write("<?php echo \"hi\\n\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(2);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second lookup");

        assert!(!first.hit);
        assert!(second.hit);
        assert_eq!(
            cache.cache_stats(),
            CompiledScriptCacheStats {
                hits: 1,
                misses: 1,
                stale_invalidations: 0,
                compile_errors: 0,
                entries: 1,
            }
        );
    }

    #[test]
    fn compiled_script_cache_invalidates_modified_script() {
        let fixture = CacheFixture::new("cache-stale");
        fixture.write("<?php echo \"one\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        fixture.write("<?php echo \"two\";");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().stale_invalidations, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &second.compiled);
        assert_eq!(output.stdout, b"two");
    }

    #[test]
    fn compiled_script_cache_separates_optimization_levels() {
        let fixture = CacheFixture::new("cache-opt-level");
        fixture.write("<?php echo \"opt\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        let first = cache
            .get_or_compile_script(
                &executor,
                PhpScriptCacheInput {
                    optimization_level: OptimizationLevel::O0,
                    ..fixture.input()
                },
            )
            .expect("first compile");
        let second = cache
            .get_or_compile_script(
                &executor,
                PhpScriptCacheInput {
                    optimization_level: OptimizationLevel::O1,
                    ..fixture.input()
                },
            )
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(
            cache.cache_stats(),
            CompiledScriptCacheStats {
                hits: 0,
                misses: 2,
                stale_invalidations: 1,
                compile_errors: 0,
                entries: 1,
            }
        );
    }

    #[test]
    fn compiled_script_cache_compile_error_does_not_poison_later_success() {
        let fixture = CacheFixture::new("cache-compile-error");
        fixture.write("<?php function {");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        assert!(matches!(
            cache.get_or_compile_script(&executor, fixture.input()),
            Err(PhpExecutionError::Compile(_))
        ));
        fixture.write("<?php echo \"ok\";");
        let lookup = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("successful compile after error");

        assert!(!lookup.hit);
        assert_eq!(cache.cache_stats().compile_errors, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &lookup.compiled);
        assert_eq!(output.stdout, b"ok");
    }

    #[test]
    fn disabled_compiled_script_cache_always_compiles() {
        let fixture = CacheFixture::new("cache-disabled");
        fixture.write("<?php echo \"hi\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::disabled();

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().hits, 0);
        assert_eq!(cache.cache_stats().misses, 2);
        assert_eq!(cache.cache_stats().entries, 0);
    }

    #[test]
    fn execute_php_renders_vm_class_table_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public function show() {} }\nclass Child extends Base {\n    protected function show() {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to child::show() must be public (as in class base) in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_property_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public static $p; }\nclass Child extends Base {\n    public $p;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Cannot redeclare static Base::$p as non static Child::$p in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_final_class_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nfinal class Base {}\nclass Child extends Base {}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class child cannot extend final class base in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_class_constant_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public const TOKEN = 1; }\nclass Child extends Base {\n    protected const TOKEN = 2;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to Child::TOKEN must be public (as in class Base) in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_interface_signature_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\ninterface Contract { public function __construct(); }\nclass Child implements Contract {\n    public function __construct($value) {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Declaration of Child::__construct($value) must be compatible with Contract::__construct() in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_direct_traversable_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass test implements Traversable {\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class test must implement interface Traversable as part of either Iterator or IteratorAggregate in fixture.php on line 2\n"
        );
    }

    #[test]
    fn execute_php_renders_invalid_const_expr_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass C { const BAD = \"$name\"; }\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Constant expression contains invalid operations in fixture.php on line 2\n"
        );
    }

    struct CacheFixture {
        path: PathBuf,
        root: PathBuf,
    }

    impl CacheFixture {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "phrust-executor-{name}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir(&root).expect("create cache fixture root");
            let path = root.join("index.php");
            Self { path, root }
        }

        fn write(&self, source: &str) {
            std::fs::write(&self.path, source).expect("write cache fixture");
        }

        fn input(&self) -> PhpScriptCacheInput {
            PhpScriptCacheInput {
                path: self.path.clone(),
                source_path: self.path.to_string_lossy().into_owned(),
                optimization_level: OptimizationLevel::O0,
            }
        }
    }

    impl Drop for CacheFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn execute_cached_for_test(
        executor: &PhpExecutor,
        compiled: &CompiledPhpScript,
    ) -> PhpExecutionOutput {
        executor.execute_compiled(
            compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli("index.php", Vec::new()),
                collect_counters: false,
            },
        )
    }
}
