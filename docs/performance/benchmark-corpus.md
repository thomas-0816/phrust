# Performance Benchmark Corpus

The Performance benchmark corpus is a small deterministic smoke set under
`tests/fixtures/performance/perf_smoke`. It is intentionally not a real-world
benchmark suite. The fixtures are designed to exercise common hot paths while
remaining stable under PHP 8.5.7 and the Rust VM baseline.

Each executable fixture has a sibling `.out` file with expected stdout. Fixtures
must not depend on networking, wall-clock time, random data, host paths, locale,
or platform-specific filesystem behavior.

## Fixtures

| Test ID | Fixture | Expected output | Hot path |
| --- | --- | --- | --- |
| `PERF-SMOKE-SMOKE-ARITHMETIC` | `arithmetic.php` | `arithmetic.php.out` | Integer arithmetic, loop-carried locals, add/sub/mul dispatch. |
| `PERF-SMOKE-SMOKE-LOOPS` | `loops.php` | `loops.php.out` | Nested `for`/`while` control flow and branch dispatch. |
| `PERF-SMOKE-SMOKE-ARRAYS-PACKED` | `arrays_packed.php` | `arrays_packed.php.out` | Packed append, foreach value iteration, `count()`. |
| `PERF-SMOKE-SMOKE-ARRAYS-MIXED` | `arrays_mixed.php` | `arrays_mixed.php.out` | String-key array reads/writes and string-key overwrite. |
| `PERF-SMOKE-SMOKE-FUNCTION-CALLS` | `function_calls.php` | `function_calls.php.out` | User function lookup, frame setup, return value flow. |
| `PERF-SMOKE-SMOKE-METHOD-CALLS` | `method_calls.php` | `method_calls.php.out` | Object construction, method dispatch, typed property read/write. |
| `PERF-SMOKE-SMOKE-PROPERTIES` | `properties.php` | `properties.php.out` | Public property read/write and integer update loop. |
| `PERF-SMOKE-SMOKE-STRINGS-CONCAT` | `strings_concat.php` | `strings_concat.php.out` | String concatenation and `strlen()` dispatch. |
| `PERF-SMOKE-SMOKE-OUTPUT-WRITES` | `output_writes.php` | `output_writes.php.out` | Multi-argument `echo`, scalar conversion, print-visible output bytes, and output-buffer flush. |
| `PERF-SMOKE-SMOKE-EXCEPTIONS-NO-THROW` | `exceptions_no_throw.php` | `exceptions_no_throw.php.out` | Try/catch region setup without throwing in the hot path. |
| `PERF-SMOKE-SMOKE-AUTOLOAD` | `autoload_smoke.php` | `autoload_smoke.php.out` | SPL autoload registration, include/require, class lookup. |

`_support/PerfAutoloadSmoke.php` is support code for
`PERF-SMOKE-SMOKE-AUTOLOAD` and is not a standalone benchmark fixture.

## Current Execution Policy

The corpus is executed by `scripts/performance/bench_matrix.py` through the just
recipes below. The standard smoke builds `php-vm`, runs the Rust VM over every
top-level fixture, records counters out-of-band, compares stdout against each
`.out` file, and writes `target/performance/benchmark-smoke.json`.

```bash
nix develop -c just benchmark-smoke
nix develop -c just hotpath-inventory
nix develop -c just perf-baseline
nix develop -c just perf-compare
nix develop -c just perf-report
```

Reference PHP runs are included when `REFERENCE_PHP` points at an executable
binary or the pinned `third_party/php-src/sapi/cli/php` exists. If no reference
binary is available, the runner records a skip reason instead of failing the
Rust VM smoke.

```bash
REFERENCE_PHP=third_party/php-src/sapi/cli/php nix develop -c just benchmark-smoke
```

The benchmark smoke uses the product-native engine. Policy behavior is covered
by `just default-profile-smoke`, `just optimizer-diff`,
`just inline-cache-model-tests`, `just cache-roundtrip`, and `just native-smoke`.

## Troubleshooting

- Flaky wall-clock timings: rerun `perf-baseline` and `perf-compare`, increase
  `PHRUST_PERF_BASELINE_REPETITIONS`, and treat wall-clock as advisory unless
  a stable CI/Callgrind budget exists.
- Output mismatches: inspect the generated stdout/stderr files under
  `target/performance`, then run the fixture directly with `target/debug/php-vm run`.
- Missing reference PHP: either set `REFERENCE_PHP` or accept the recorded
  reference skip; Rust VM fixture correctness still runs.
- Missing optional profiling tools: `callgrind-smoke` skips on
  Darwin or without Valgrind. That skip is expected for standard local gates.
