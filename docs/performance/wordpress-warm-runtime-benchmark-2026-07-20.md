# WordPress warm-runtime benchmark, 2026-07-20

This report records a fresh, instrumentation-free WordPress comparison at
commit `dd1bd85f`. A separate diagnostic request supplies runtime counters;
its wall time is intentionally excluded from the benchmark result. Raw
artifacts remain under `target/performance/` and are not committed.

## Result

Phrust remains CPU-bound and substantially slower than PHP-FPM 8.5.7:

| Metric | Phrust | PHP-FPM 8.5.7 | Phrust / PHP |
| --- | ---: | ---: | ---: |
| p50 latency | 443.62 ms | 29.49 ms | 15.04x |
| p95 latency | 464.12 ms | 37.49 ms | 12.38x |
| Throughput, concurrency 1 | 2.27 req/s | 32.83 req/s | 0.069x |
| CPU, 20 measured requests | 8.57 s | 0.53 s | 16.17x |
| Peak RSS | 681.7 MB | 140.1 MB | 4.87x |

The p50 bootstrap intervals were 426.13-448.76 ms for Phrust and
26.98-32.14 ms for PHP-FPM. Phrust's measured range was 404.93-464.93 ms.

Every correctness probe passed. Both engines returned HTTP 200, the selected
headers matched, and both produced the same 70,949-byte body with SHA-256:

```text
7a34e150c5304aea4744a0e4f3b4fd70c4309cca411902513478a9ba7a196072
```

## Comparison with the 2026-07-19 snapshot

The preceding documented snapshot measured 440.72 ms p50. The new 443.62 ms
result is 0.66% slower and lies within normal host variation. It is not a
performance improvement.

Several internal traffic metrics did improve substantially:

| Metric | 2026-07-19 | 2026-07-20 | Change |
| --- | ---: | ---: | ---: |
| Warm p50 | 440.72 ms | 443.62 ms | +0.66% |
| Warm p95 | 499.36 ms | 464.12 ms | -7.06% |
| Runtime helper calls | 1,323,262 | 908,306 | -31.36% |
| Value-table allocations | 484,557 | 184,840 | -61.85% |
| Value-table high-water | 251,855 | 93,431 | -62.90% |
| Value-release helpers | 245,687 | 36,230 | -85.25% |
| Native call-frame bytes | 35.36 MB | 2.89 MB | -91.83% |

The old and new diagnostic schemas describe the same broad request but span
multiple runtime architecture tranches. They are useful for direction, not
for attributing every reduction to commit `dd1bd85f`.

The important conclusion is that reducing transport counters alone did not
reduce wall time. The remaining operations still cross into generic Rust
runtime code and manipulate `Value`, arrays, objects, strings, and ownership
there. A smaller call frame does not help enough when the callee still performs
the same generic work.

## Current warm runtime volume

The separate diagnostic request recorded zero compilation attempts, zero
compile time, and zero published versions. Warm compilation is therefore not
part of the 443.62 ms result.

The request crossed 908,306 runtime-helper boundaries:

| Helper group | Calls |
| --- | ---: |
| Arrays and foreach | 404,402 |
| Scalar, comparison, cast, truthy, and string predicates | 154,536 |
| Locals, references, globals, and lifecycle | 133,842 |
| Properties | 109,262 |
| Calls, return checks, and argument checks | 61,904 |
| Remaining semantic and support helpers | 44,360 |

The largest individual helper counts were:

| Helper | Calls |
| --- | ---: |
| Array fetch | 211,172 |
| Array insert | 118,573 |
| Reference bind | 57,473 |
| Property fetch | 55,923 |
| Binary operation | 53,166 |
| Direct builtin call | 51,742 |
| Cast | 40,291 |
| Value release | 36,230 |
| Compare | 33,405 |
| Array creation | 32,012 |
| Local fetch | 29,114 |
| Semantic property | 27,472 |
| Foreach initialization | 22,632 |
| Property assignment | 20,498 |

The helper timer attributed 408.2 ms to helper bodies during the diagnostic
request. That run took about 1.46 seconds of native execution because timing
and attribution were enabled; helper timings are ranking data, not clean wall
time. The largest timed families were direct builtins, array insertion, array
fetch, property fetch, semantic properties, foreach initialization, binary
operations, and reference binding.

## Value and call planes

One diagnostic request recorded:

- 184,840 value-table allocations;
- 34,799 slot reuses;
- a 93,431-slot high-water mark;
- 36,230 release helpers, with 34,934 reaching zero;
- 7,543 ownership escapes;
- 2,887,536 native call-frame bytes; and
- 61,544 separately allocated call-argument bytes.

The call plane reported 99,513 direct calls and only 119 genuinely dynamic
calls. It also executed 24,270 same-unit direct calls, 4,275 cross-unit direct
calls, and 20,754 of 22,002 eligible monomorphic method calls.

Call resolution is therefore no longer the primary explanation for the warm
gap. Ordinary PHP data operations dominate: arrays and foreach alone account
for 44.5% of all remaining helper boundaries, and properties add another
12.0%.

The most frequent materializations reinforce that conclusion:

| Materialization origin | Count |
| --- | ---: |
| Array from array creation | 32,012 |
| String from binary operation | 30,341 |
| Array iterator from foreach initialization | 22,544 |
| Reference from reference binding | 22,187 |
| String from direct builtin | 20,246 |
| Array from direct builtin | 20,021 |
| Array from array insertion | 17,589 |
| String from array fetch | 8,876 |
| Array from property fetch | 7,986 |

## Measurement conditions

- Host: Linux x86_64, Intel Core i7-12800H, 20 logical CPUs.
- Source commit: `dd1bd85ffe6ece458828cfa74f1eafc56561dfaf`.
- WordPress: 6.8.3, tree
  `b45a7f2bf74279f41671118f8de42371e6c581f32e0c541adfc670b771ca0ce9`.
- Reference: PHP-FPM 8.5.7 with Opcache enabled and Opcache JIT disabled.
- Phrust: release build with `--no-default-features`, default engine preset,
  preloaded script cache, and warm in-memory native code.
- Load: concurrency 1, five warmups, twenty measured requests per engine.
- Clean timing used external process sampling with runtime counters disabled.
- Diagnostic counters came from a separate one-sample request.

Reproduction commands:

```bash
nix develop -c cargo build --release \
  -p php_server --bin phrust-server --no-default-features

nix develop -c scripts/performance/wordpress_root_benchmark.py \
  --mode clean \
  --wordpress-dir target/wordpress-cutover \
  --docroot target/wordpress-cutover \
  --server target/release/phrust-server \
  --host-header 127.0.0.1:37023 \
  --samples 20 \
  --warmups 5 \
  --concurrency 1 \
  --timeout-seconds 60 \
  --out-dir target/performance/docs-2026-07-20-clean

nix develop -c scripts/performance/wordpress_root_benchmark.py \
  --mode diagnostic \
  --wordpress-dir target/wordpress-cutover \
  --docroot target/wordpress-cutover \
  --server target/release/phrust-server \
  --host-header 127.0.0.1:37023 \
  --samples 1 \
  --warmups 1 \
  --concurrency 1 \
  --timeout-seconds 60 \
  --out-dir target/performance/docs-2026-07-20-diagnostic
```

## Verdict

The latest runtime work removed substantial helper, release, allocation, and
call-transport volume without improving clean p50. This rules out further
transport-only cleanup as the main strategy.

The next performance tranche must remove the generic runtime boundary from a
large shared data path. The highest-leverage target is a Cranelift-visible
native value plane for arrays, foreach state, and declared properties, with
typed out-of-line helpers reserved for PHP-visible exceptional cases. An
acceptable tranche must reduce wall time together with helper count and value
traffic; counter reductions without a clean latency reduction are not a
performance win.
