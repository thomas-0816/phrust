use criterion::{Criterion, black_box, criterion_group, criterion_main};
use php_ir::{LoweringOptions, lower_frontend_result};
use php_lexer::{LexerConfig, lex_all};
use php_runtime::api::{ArrayKey, PhpArray, PhpString, Value};
use php_semantics::analyze_source;
use php_syntax::parse_source_file;
use php_vm::api::{CompiledUnit, InlineCacheMode, QuickeningMode, Vm, VmOptions};
use std::time::Duration;

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

    c.bench_function("performance/vm_dispatch_micro_loop", |b| {
        b.iter(|| execute_unit(black_box(&loop_unit)));
    });
    c.bench_function("performance/function_call_dispatch", |b| {
        b.iter(|| execute_unit(black_box(&call_unit)));
    });
    c.bench_function("performance/property_lookup", |b| {
        b.iter(|| execute_unit(black_box(&property_unit)));
    });
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

    let mut mixed = PhpArray::new();
    for index in 0..64 {
        mixed.insert(ArrayKey::Int(index), Value::Int(index));
        mixed.insert(
            ArrayKey::String(PhpString::from(format!("key{index}").as_str())),
            Value::Int(index),
        );
    }
    let mixed_key = ArrayKey::String(PhpString::from("key37"));
    c.bench_function("performance/mixed_array_access", |b| {
        b.iter(|| {
            let int_value = mixed.get(black_box(&ArrayKey::Int(37)));
            let string_value = mixed.get(black_box(&mixed_key));
            black_box((int_value, string_value));
        });
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

criterion_group! {
    name = perf_hotpaths;
    config = configured_criterion();
    targets = bench_frontend, bench_vm, bench_runtime
}
criterion_main!(perf_hotpaths);
