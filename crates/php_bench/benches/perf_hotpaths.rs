use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use php_ir::{LoweringOptions, lower_frontend_result};
use php_lexer::{LexerConfig, lex_all};
use php_optimizer::OptimizationLevel;
use php_runtime::api::{ArrayKey, PhpArray, PhpString, Value};
use php_runtime::builtins::string_intrinsics;
use php_semantics::analyze_source;
use php_source::byte_kernel;
use php_syntax::parse_source_file;
use php_vm::api::{
    CompiledUnit, DeploymentRootFingerprint, DeploymentRootMode, IncludeCache, IncludeLoader,
    InlineCacheMode, QuickeningMode, Vm, VmOptions,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FRONTEND_SOURCE: &str = r#"<?php
function perf_add($a, $b) {
    return $a + $b;
}

class PerfBox {
    public $value = 7;

    public function value() {
        return $this->value;
    }
}

$box = new PerfBox();
$sum = 0;
for ($i = 0; $i < 32; $i++) {
    $sum = perf_add($sum, $box->value());
}
echo $sum;
"#;

const VM_LOOP_SOURCE: &str = r#"<?php
$sum = 0;
for ($i = 0; $i < 64; $i++) {
    $sum = $sum + $i;
}
echo $sum;
"#;

const VM_CALL_SOURCE: &str = r#"<?php
function perf_call($a, $b) {
    return $a + $b;
}

$sum = 0;
for ($i = 0; $i < 48; $i++) {
    $sum = perf_call($sum, $i);
}
echo $sum;
"#;

const VM_PROPERTY_SOURCE: &str = r#"<?php
class PerfPropertyBox {
    public $value = 3;

    public function get() {
        return $this->value;
    }
}

$box = new PerfPropertyBox();
$sum = 0;
for ($i = 0; $i < 48; $i++) {
    $sum = $sum + $box->value + $box->get();
}
echo $sum;
"#;

const VM_BUILTIN_MIX_SOURCE: &str = r#"<?php
$sum = 0;
for ($i = 0; $i < 64; $i++) {
    $text = trim(strtolower('  WordPress-Plugin-Route  '));
    $parts = explode('-', $text);
    $sum += strlen($text) + count($parts);
    $sum += function_exists('strlen') ? 1 : 0;
    $sum += defined('PHP_VERSION') ? 1 : 0;
}
echo $sum;
"#;

fn configured_criterion() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(50))
        .measurement_time(Duration::from_millis(200))
}

fn compile_unit(source: &str) -> CompiledUnit {
    let frontend = analyze_source(source);
    assert!(
        !frontend.has_errors(),
        "benchmark source must analyze without errors"
    );
    let lowered = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_text: Some(source.to_owned()),
            ..LoweringOptions::default()
        },
    );
    assert!(
        lowered.diagnostics.is_empty(),
        "benchmark source must lower without diagnostics: {:?}",
        lowered.diagnostics
    );
    CompiledUnit::new(lowered.unit)
}

fn execute_unit(unit: &CompiledUnit) {
    let vm = Vm::with_options(VmOptions {
        verify_ir: false,
        quickening: QuickeningMode::On,
        inline_caches: InlineCacheMode::On,
        ..VmOptions::default()
    });
    let result = vm.execute(unit.clone());
    assert!(result.status.is_success(), "{:?}", result.status);
    black_box(result.output);
}

struct IncludeBenchmarkFixture {
    root: PathBuf,
}

impl IncludeBenchmarkFixture {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "phrust-include-bench-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos()),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create include benchmark root");
        std::fs::write(
            root.join("lib.php"),
            "<?php function include_benchmark_target($value) { return $value + 1; }\n",
        )
        .expect("write include benchmark source");
        std::fs::write(
            root.join("Registry.php"),
            "<?php namespace Bench; use Bench\\Traits\\SharedTrait; class Registry { use SharedTrait; }\n",
        )
        .expect("write multi-file include benchmark root");
        std::fs::create_dir_all(root.join("Traits"))
            .expect("create multi-file include benchmark dependency directory");
        std::fs::write(
            root.join("Traits/SharedTrait.php"),
            "<?php namespace Bench\\Traits; trait SharedTrait { private $value = 1; }\n",
        )
        .expect("write multi-file include benchmark dependency");
        Self { root }
    }
}

impl Drop for IncludeBenchmarkFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn bench_include_cache_identity(c: &mut Criterion) {
    let fixture = IncludeBenchmarkFixture::new();
    let loader = IncludeLoader::for_root(&fixture.root).expect("include loader");

    let mutable_cache = IncludeCache::new(4);
    let mutable_resolved = mutable_cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve mutable include");
    mutable_cache
        .get_or_compile_include(&loader, &mutable_resolved, OptimizationLevel::O0)
        .expect("warm mutable include");
    c.bench_function("performance/include_cache_hit_mutable_content", |b| {
        b.iter(|| {
            black_box(
                mutable_cache
                    .get_or_compile_include(
                        black_box(&loader),
                        black_box(&mutable_resolved),
                        OptimizationLevel::O0,
                    )
                    .expect("mutable include hit"),
            );
        });
    });
    let mutable_stats = mutable_cache.cache_stats();

    let immutable_cache = IncludeCache::new(4);
    immutable_cache.set_deployment_root_fingerprint(DeploymentRootFingerprint::observe(
        &fixture.root,
        DeploymentRootMode::ImmutableDeclared,
    ));
    let immutable_resolved = immutable_cache
        .resolve_with_include_path(&loader, None, "lib.php", &[], Some(&fixture.root))
        .expect("resolve immutable include");
    immutable_cache
        .get_or_compile_include(&loader, &immutable_resolved, OptimizationLevel::O0)
        .expect("warm immutable include");
    c.bench_function("performance/include_cache_hit_immutable_identity", |b| {
        b.iter(|| {
            black_box(
                immutable_cache
                    .get_or_compile_include(
                        black_box(&loader),
                        black_box(&immutable_resolved),
                        OptimizationLevel::O0,
                    )
                    .expect("immutable include hit"),
            );
        });
    });
    let immutable_stats = immutable_cache.cache_stats();

    eprintln!(
        "include-cache counters: mutable validations={} bytes_hashed={} identity_hits={}; immutable validations={} bytes_hashed={} identity_hits={}",
        mutable_stats.content_validations,
        mutable_stats.source_bytes_hashed,
        mutable_stats.identity_only_hits,
        immutable_stats.content_validations,
        immutable_stats.source_bytes_hashed,
        immutable_stats.identity_only_hits,
    );
}

fn bench_multi_file_include_compile(c: &mut Criterion) {
    let fixture = IncludeBenchmarkFixture::new();
    let loader = IncludeLoader::for_root(&fixture.root)
        .expect("include loader")
        .with_compilation_dependency("Bench\\Traits\\SharedTrait", "Traits/SharedTrait.php");
    let resolved = IncludeCache::new(1)
        .resolve_with_include_path(&loader, None, "Registry.php", &[], Some(&fixture.root))
        .expect("resolve multi-file include");

    c.bench_function("performance/multi_file_trait_compile", |b| {
        b.iter(|| {
            let cache = IncludeCache::new(1);
            black_box(
                cache
                    .get_or_compile_include(
                        black_box(&loader),
                        black_box(&resolved),
                        OptimizationLevel::O0,
                    )
                    .expect("compile multi-file include"),
            );
        });
    });
}

fn bench_frontend(c: &mut Criterion) {
    let parser_source = FRONTEND_SOURCE.repeat(16);
    c.bench_function("performance/lexer_parser_smoke", |b| {
        b.iter(|| {
            let lexed = lex_all(black_box(&parser_source), LexerConfig::default());
            black_box(lexed.tokens.len());
            let parsed = parse_source_file(black_box(parser_source.as_str()));
            black_box(parsed.diagnostics().len());
        });
    });

    c.bench_function("performance/ir_lower_smoke", |b| {
        b.iter(|| {
            let frontend = analyze_source(black_box(FRONTEND_SOURCE));
            let lowered = lower_frontend_result(
                black_box(&frontend),
                LoweringOptions {
                    source_text: Some(FRONTEND_SOURCE.to_owned()),
                    ..LoweringOptions::default()
                },
            );
            black_box(lowered.unit.functions.len());
        });
    });
}

fn bench_vm(c: &mut Criterion) {
    let loop_unit = compile_unit(VM_LOOP_SOURCE);
    let call_unit = compile_unit(VM_CALL_SOURCE);
    let property_unit = compile_unit(VM_PROPERTY_SOURCE);
    let builtin_mix_unit = compile_unit(VM_BUILTIN_MIX_SOURCE);

    c.bench_function("performance/vm_dispatch_micro_loop", |b| {
        b.iter(|| execute_unit(black_box(&loop_unit)));
    });
    c.bench_function("performance/function_call_dispatch", |b| {
        b.iter(|| execute_unit(black_box(&call_unit)));
    });
    c.bench_function("performance/property_lookup", |b| {
        b.iter(|| execute_unit(black_box(&property_unit)));
    });
    c.bench_function("performance/builtin_context_arginfo_mix", |b| {
        b.iter(|| execute_unit(black_box(&builtin_mix_unit)));
    });
}

fn dynamic_symbol_source(symbols: usize) -> String {
    let mut source = String::from("<?php\nif (true) {\n");
    for index in 0..symbols {
        source.push_str(&format!("function dynamic_symbol_{index}() {{ return {index}; }}\n"));
    }
    source.push_str(&format!(
        "}}\necho dynamic_symbol_0() + dynamic_symbol_{}();\n",
        symbols - 1
    ));
    source
}

fn bench_dynamic_symbols(c: &mut Criterion) {
    let units = [128usize, 1_024, 4_096]
        .into_iter()
        .map(|symbols| (symbols, compile_unit(&dynamic_symbol_source(symbols))))
        .collect::<Vec<_>>();
    let mut group = c.benchmark_group("performance/dynamic_symbol_registration_lookup");
    for (symbols, unit) in &units {
        group.bench_with_input(BenchmarkId::from_parameter(symbols), unit, |b, unit| {
            b.iter(|| execute_unit(black_box(unit)));
        });
    }
    group.finish();
}

fn mixed_string_array(size: i64) -> PhpArray {
    let mut array = PhpArray::new();
    for index in 0..size {
        array.insert(
            ArrayKey::String(PhpString::from_bytes(format!("key-{index}").into_bytes())),
            Value::Int(index),
        );
    }
    array
}

fn mixed_key(index: i64) -> ArrayKey {
    ArrayKey::String(PhpString::from_bytes(
        format!("key-{index}").into_bytes(),
    ))
}

fn bench_runtime(c: &mut Criterion) {
    let packed = PhpArray::from_packed((0..128).map(Value::Int).collect());
    c.bench_function("performance/packed_array_access", |b| {
        b.iter(|| {
            let mut sum = 0i64;
            for index in 0..128 {
                if let Some(Value::Int(value)) = packed.packed_element_fast(black_box(index)) {
                    sum += value;
                }
            }
            black_box(sum);
        });
    });
    c.bench_function("performance/packed_array_iteration", |b| {
        b.iter(|| black_box(packed.iter().count()));
    });

    let mut mixed = PhpArray::new();
    for index in 0..64 {
        mixed.insert(ArrayKey::Int(index), Value::Int(index));
        mixed.insert(
            ArrayKey::String(PhpString::from(format!("key{index}").as_str())),
            Value::Int(index),
        );
    }
    let mixed_access_key = ArrayKey::String(PhpString::from("key37"));
    c.bench_function("performance/mixed_array_access", |b| {
        b.iter(|| {
            let int_value = mixed.get(black_box(&ArrayKey::Int(37)));
            let string_value = mixed.get(black_box(&mixed_access_key));
            black_box((int_value, string_value));
        });
    });

    let mut delete_group = c.benchmark_group("performance/mixed_array_delete");
    for size in [128i64, 1_024, 4_096] {
        delete_group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, size| {
            b.iter_batched(
                || mixed_string_array(*size),
                |mut array| {
                    for step in 0..(*size / 2) {
                        let index = (step * 73) % *size;
                        black_box(array.remove(&mixed_key(index)));
                    }
                    black_box(array);
                },
                BatchSize::SmallInput,
            );
        });
    }
    delete_group.finish();

    let mut reinsert_group = c.benchmark_group("performance/mixed_array_delete_reinsert");
    for size in [128i64, 1_024, 4_096] {
        reinsert_group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, size| {
            b.iter_batched(
                || mixed_string_array(*size),
                |mut array| {
                    for step in 0..(*size / 4) {
                        let index = (step * 73) % *size;
                        let key = mixed_key(index);
                        black_box(array.remove(&key));
                        array.insert(key, Value::Int(index + *size));
                    }
                    black_box(array);
                },
                BatchSize::SmallInput,
            );
        });
    }
    reinsert_group.finish();

    c.bench_function("performance/mixed_array_compaction_threshold", |b| {
        b.iter_batched(
            || {
                let mut array = mixed_string_array(4_096);
                for step in 0..2_047 {
                    array.remove(&mixed_key((step * 73) % 4_096));
                }
                array
            },
            |mut array| {
                black_box(array.remove(&mixed_key((2_047 * 73) % 4_096)));
                black_box(array);
            },
            BatchSize::SmallInput,
        );
    });

    let mut tombstones = mixed_string_array(4_096);
    for index in (0..4_096).step_by(3) {
        tombstones.remove(&mixed_key(index));
    }
    c.bench_function("performance/mixed_array_iterate_tombstones", |b| {
        b.iter(|| black_box(tombstones.iter().count()));
    });

    c.bench_function("performance/string_concat_builder", |b| {
        b.iter(|| {
            let mut value = PhpString::from_bytes(Vec::with_capacity(384));
            for _ in 0..64 {
                value.bytes_mut().extend_from_slice(black_box(b"abc"));
            }
            black_box(value.len());
        });
    });
}

fn bench_byte_kernels(c: &mut Criterion) {
    let ascii_text =
        b"Alpha_beta_123 plain text with spaces and punctuation. ".repeat(256);
    let mut json_text =
        b"plain_ascii_payload_with_no_escapes_or_unicode_".repeat(256);
    json_text.extend_from_slice(br#"needs " escaping after a long prefix"#);
    let html_text =
        b"plain html content with no entities until a late <tag> & quote ".repeat(192);
    let mut trim_text = Vec::with_capacity(8192);
    trim_text.extend(std::iter::repeat_n(b' ', 256));
    trim_text.extend_from_slice(&ascii_text);
    trim_text.extend(std::iter::repeat_n(b'\n', 256));

    c.bench_function("performance/byte_kernel_find_subslice", |b| {
        b.iter(|| {
            black_box(byte_kernel::find_bytes(
                black_box(&ascii_text),
                black_box(b"punctuation"),
            ));
        });
    });

    c.bench_function("performance/byte_kernel_rfind_subslice", |b| {
        b.iter(|| {
            black_box(byte_kernel::rfind_bytes_before(
                black_box(&ascii_text),
                black_box(b"plain"),
                black_box(ascii_text.len()),
            ));
        });
    });

    c.bench_function("performance/byte_kernel_ascii_ci_find_subslice", |b| {
        b.iter(|| {
            black_box(byte_kernel::find_bytes_ascii_case_insensitive_from(
                black_box(&ascii_text),
                black_box(b"PUNCTUATION"),
                black_box(0),
            ));
        });
    });

    c.bench_function("performance/byte_kernel_ascii_ci_rfind_subslice", |b| {
        b.iter(|| {
            black_box(byte_kernel::rfind_bytes_ascii_case_insensitive_before(
                black_box(&ascii_text),
                black_box(b"ALPHA"),
                black_box(ascii_text.len()),
            ));
        });
    });

    c.bench_function("performance/byte_kernel_count_byte", |b| {
        b.iter(|| {
            black_box(byte_kernel::count_byte(black_box(&ascii_text), black_box(b' ')));
        });
    });

    c.bench_function("performance/byte_kernel_digit_run", |b| {
        let digits = b"1234567890".repeat(1024);
        b.iter(|| {
            black_box(byte_kernel::ascii_digit_run_len(black_box(&digits)));
        });
    });

    c.bench_function("performance/byte_kernel_whitespace_scan", |b| {
        let mut spaced = b" \t\n\r\x0c".repeat(1024);
        spaced.extend_from_slice(b"content");
        b.iter(|| {
            black_box(byte_kernel::find_non_ascii_whitespace(black_box(&spaced)));
            black_box(byte_kernel::rfind_ascii_whitespace(black_box(&spaced)));
        });
    });

    c.bench_function("performance/byte_kernel_json_escape_scan", |b| {
        b.iter(|| {
            black_box(byte_kernel::find_json_escape_byte(black_box(&json_text)));
        });
    });

    c.bench_function("performance/byte_kernel_html_escape_scan", |b| {
        b.iter(|| {
            black_box(byte_kernel::find_html_escape_byte(black_box(&html_text)));
        });
    });

    c.bench_function("performance/byte_kernel_ascii_uppercase_copy", |b| {
        b.iter(|| {
            black_box(byte_kernel::ascii_uppercase_copy(black_box(&ascii_text)));
        });
    });

    c.bench_function("performance/byte_kernel_default_trim_bounds", |b| {
        b.iter(|| {
            black_box(byte_kernel::trim_default_bounds(black_box(&trim_text)));
        });
    });
}

fn bench_string_intrinsics(c: &mut Criterion) {
    let mixed_case = PhpString::from_bytes(
        b"Alpha Beta Gamma Delta epsilon zeta eta theta ".repeat(256),
    );
    let html = PhpString::from_bytes(
        b"plain html until the last chunk contains <tag attr=\"value\"> & text"
            .repeat(192),
    );
    let exploded = PhpString::from_bytes(b"field,".repeat(512));
    let mut trim_bytes = Vec::with_capacity(8192);
    trim_bytes.extend(std::iter::repeat_n(b' ', 128));
    trim_bytes.extend_from_slice(b"content with default whitespace trimming");
    trim_bytes.extend(std::iter::repeat_n(b'\t', 128));
    let trim = PhpString::from_bytes(trim_bytes);

    c.bench_function("performance/string_intrinsic_strtolower", |b| {
        b.iter(|| {
            black_box(string_intrinsics::strtolower_ascii(black_box(&mixed_case)));
        });
    });

    c.bench_function("performance/string_intrinsic_htmlspecialchars", |b| {
        b.iter(|| {
            black_box(string_intrinsics::htmlspecialchars_default(black_box(&html)));
        });
    });

    c.bench_function("performance/string_intrinsic_explode_single_byte", |b| {
        b.iter(|| {
            black_box(string_intrinsics::explode_single_byte(
                black_box(b','),
                black_box(&exploded),
            ));
        });
    });

    c.bench_function("performance/string_intrinsic_trim_default", |b| {
        b.iter(|| {
            black_box(string_intrinsics::trim_ascii_default(black_box(&trim)));
        });
    });
}

criterion_group! {
    name = perf_hotpaths;
    config = configured_criterion();
    targets = bench_frontend, bench_vm, bench_dynamic_symbols, bench_runtime, bench_byte_kernels, bench_string_intrinsics, bench_include_cache_identity, bench_multi_file_include_compile
}
criterion_main!(perf_hotpaths);
