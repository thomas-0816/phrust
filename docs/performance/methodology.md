# Performance Performance Methodology

Performance performance work is measured with reproducible local tools from the Nix
dev shell. Performance data is useful only when it is paired with correctness
checks and enough environment detail to reproduce the run.

## Dev Shell Tools

| Tool | Platform | Purpose | Required for standard gates |
| --- | --- | --- | --- |
| `cargo` / `rustc` / `rustfmt` / `clippy` | Linux through Nix; Darwin from the host | Rust build, test, format, and lint workflow | yes |
| `cargo-nextest` | Linux through Nix | Optional faster Rust test runner for later Performance gates | no |
| `hyperfine` | Linux and Darwin through Nix | CLI wall-clock benchmark smokes and local before/after comparisons | no |
| `jq` | Linux and Darwin through Nix | JSON normalization, report inspection, and shell-script assertions | yes |
| `python3` | Linux and Darwin through Nix | Deterministic benchmark, diff, and report scripts using the standard library | yes |
| `sccache` | Linux and Darwin through Nix | Rust compilation cache through `RUSTC_WRAPPER` | no, but enabled when available |
| `ccache` | Linux and Darwin through Nix | C/C++ compilation cache for reference and native build helpers | no, but enabled when available |
| `mold` | Linux only | Faster linker for Linux Rust builds through `RUSTFLAGS` | no |
| `valgrind` | Linux only | Callgrind/Cachegrind instruction-count and cache smokes | no |
| `perf` / `linuxPackages.perf` | Linux only | Optional local CPU profiling and counter exploration | no |
| `gdb` | Linux only | Local runtime debugging | no |
| `shellcheck` | Linux through Nix; Darwin from the host when installed | Optional script linting for older verification helpers | no |
| native PHP/C build tools | Linux through Nix; Darwin from the host when needed | Optional reference PHP and native-library workflows | no |

Linux-only tools are added with Nix conditionals so Darwin shells and `nix
flake check` do not evaluate them as required Darwin packages. The default
Darwin shell uses `mkShellNoCC` plus a lightweight Nix tool surface for `just`,
`jq`, `hyperfine`, `ripgrep`, `fd`, `python3`, `ccache`, and `sccache`, while
keeping the existing host Rust toolchain stable. Darwin shells intentionally do
not pull `shellcheck` from Nix because that requires a large Haskell closure on
current nixpkgs; script linting remains an optional host-tool check there.

## Environment Normalization

Benchmark scripts added during Performance should set or record:

- `TZ=UTC`
- `LC_ALL=C`
- deterministic temporary directories under `target/performance`
- deterministic seeds for generated fixtures
- engine version, PHP target version, target triple, and relevant feature flags
- cache directories used by bytecode-cache or compiler tooling

Wall-clock measurements must be reported as advisory unless paired with stable
fixtures, repeated runs, and documented uncertainty.

## Correctness Before Measurement

Every measured optimized mode must have a comparable baseline mode. The baseline
is `--opt-level=0` plus disabled quickening, inline caches, bytecode cache, and
JIT once those flags exist. A performance result is not actionable if the A/B
correctness comparison fails.

## Application-Flow Suite

`docs/performance/app-flows.md` describes the application-flow performance
suite. It runs ten deterministic PHP application-style fixtures through Phrust
baseline/fast rows and, when available, the pinned reference PHP CLI. The suite
is intended to complement microbenchmark and framework-like smoke tests with
request-shaped flows covering routing, service lookup, templating, validation,
hydration, collections, middleware, auth policy, and translation lookup.

Wall-clock measurements from this suite are advisory host-local trend data.
Correctness, manifest/schema validity, and Phrust/reference equivalence are the
gate conditions; speed ratios are reported, not enforced.

## Validation Commands

Standard Performance workflow:

```bash
nix develop -c just verify-performance
nix develop -c just performance-regression
nix develop -c just perf-flag-matrix
nix develop -c just benchmark-smoke
nix develop -c just perf-baseline
nix develop -c just perf-compare
nix develop -c just perf-report
```

Layer-specific gates:

```bash
nix develop -c just cache-roundtrip
nix develop -c just optimizer-diff
nix develop -c just quickening-smoke
nix develop -c just inline-cache-smoke
nix develop -c just jit-smoke
nix develop -c just safety-audit-smoke
```

Work item validates the shell surface with:

```bash
nix develop -c just --list
nix develop -c cargo --version
nix develop -c jq --version
nix develop -c hyperfine --version
```

Linux maintainers can additionally inspect optional tools with:

```bash
nix develop -c valgrind --version
nix develop -c perf --version
```

Those Linux commands are not required on Darwin.

## CI Policy

Performance CI is documented in `docs/performance/ci-policy.md`. Pull-request CI runs
the lightweight flake metadata check and the same required gate maintainers run
locally:

```bash
nix flake check
nix develop -c just verify-performance
```

Long benchmark suites are manual or scheduled only:

```bash
nix develop -c just benchmark-suite
nix develop -c just perf-report
```

The required CI path must not depend on secrets, network access from tests,
native JIT support, or optional profiling tools. JIT and profiling paths must
skip or fall back with explicit reasons when unsupported.

## Performance Ratchet

`docs/performance/ratchet.md` describes the ratchet workflow for local
before/after decisions:

```bash
nix develop -c just perf-ratchet-smoke
nix develop -c just perf-ratchet-baseline
nix develop -c just perf-ratchet-current
nix develop -c just perf-ratchet-compare
nix develop -c just perf-ratchet-next-prompt
```

The ratchet separates smoke, full, and strict modes. Smoke mode validates the
artifact schema and correctness cheaply. Full mode collects local decision data.
Strict mode may promote deterministic counter and repeated large wall-clock
regressions to failures. Raw ratchet artifacts remain under
`target/performance/ratchet/`.

## Optional Callgrind Smoke

Work item adds `just callgrind-smoke` for instruction-count
smoke measurements. The gate is intentionally optional:

- non-Linux hosts skip with a recorded reason in
  `target/performance/callgrind/summary.json`;
- Linux hosts without `valgrind` in `PATH` also skip cleanly;
- Linux hosts with Valgrind run three small CLI scenarios from
  `tests/fixtures/performance/perf_smoke/` under `--tool=callgrind`;
- outputs are still compared to fixture expectations before instruction counts
  are accepted;
- no strict instruction thresholds are enforced until a stable CI environment is
  dedicated to those counters.

The summary files are local artifacts under `target/performance/callgrind/` and are
not committed.

## Criterion Rust Hot-Path Benchmarks

Work item adds a benchmark-only in-repository package, `php_bench`, with
Criterion as a dev-dependency. It is excluded from the main workspace so
`cargo test --workspace` and `verify-performance` do not compile Criterion.
Engine/runtime crates do not depend on Criterion. The `just rust-hotpath-bench`
recipe runs deterministic Rust-level hot-path cases:

- lexer plus parser smoke;
- semantic frontend to IR lowering;
- VM dispatch loop;
- user function call dispatch;
- property lookup through the VM;
- packed array access;
- mixed array access;
- PHP string buffer growth.

These benchmarks are local trend indicators. They are not compatibility
evidence by themselves and must be paired with `verify-performance` or a narrower
correctness gate before optimization claims are accepted.

## Performance Report

Work item adds `just perf-report`, which renders
`target/performance/perf-report.md` and `target/performance/perf-report.json` from the
latest benchmark JSON, usually `target/performance/benchmark-smoke.json`. The
report includes environment metadata, commit/version, optimization flags,
scenario status, counter hotspots, cache hit/miss counters, quickening counters,
inline-cache counters, and the current Performance known-gap table.

The report is intentionally local and host-specific, so generated files remain
under `target/performance`. It does not compare wall-clock timings by itself. Create
a local baseline with `nix develop -c just perf-baseline`, compare with
`nix develop -c just perf-compare`, and use the report as a readable index into
those artifacts.

If benchmark JSON is missing, `perf-report` still writes a readable report with
a missing-data section and the commands needed to create inputs.

## Failure Policy

- Missing required all-platform tools are a dev-shell failure.
- Missing Linux-only tools on Darwin are expected.
- Missing optional profiling tools should make optional profiling gates skip
  clearly, not fail the standard verification path.
- No shell hook may download unpinned binaries or mutate global tool state.

## Troubleshooting

- Flaky benchmarks: rerun with larger `PHRUST_PERF_BASELINE_REPETITIONS` or
  `PHRUST_PERF_BENCH_SMOKE_REPETITIONS`, compare trends rather than a single
  wall-clock sample, and keep `verify-performance` as the correctness source.
- Missing Valgrind or `perf`: `callgrind-smoke` skips on Darwin or
  without Valgrind. Linux maintainers can install/use those tools through the
  Nix shell and rerun the optional gate.
- Unsupported JIT platform: keep `--jit=off` for standard runs. Use
  `jit-smoke` for the supported default-off and feature-on proof path; do not
  claim production JIT readiness.
- Cache invalidation failures: run `cache-roundtrip`, use
  `--bytecode-cache-stats`, clear the local cache directory, and inspect
  fingerprint dimensions before changing cache semantics.
- Output diffs: inspect the per-gate artifacts under `target/performance`, rerun the
  single fixture with `target/debug/php-vm run`, and compare against the
  baseline flags from `perf-flag-matrix`.

## Performance JSON Format

Work item adds `crates/php_perf`, a data-model crate for normalized Performance
performance JSON. The crate does not execute benchmarks. It defines:

- `PerfRunId`: stable run identifier.
- `PerfEnvironment`: engine version, optional git commit, Rust target triple,
  optimization flags, feature flags, and normalized environment fields.
- `PerfScenario`: stable scenario id, human name, group, and optional fixture.
- `PerfMetric`: named numeric metric with unit and directionality.
- `PerfMeasurement`: scenario, iterations, metrics, optional wall time, and
  optional VM counters.
- `PerfReport`: schema version, run id, environment, and measurements.
- `PhaseTimingReport`: stable phase-timing sidecar for `php-vm run` and
  `php-vm compile`.

Reports use serde JSON with normalized pretty output and a trailing newline via
`PerfReport::to_stable_json()`. Maps use sorted `BTreeMap` storage so feature
flags, extra environment fields, and VM counters are deterministic.

Minimal report shape:

```json
{
  "schema_version": 1,
  "run_id": "performance-test-run",
  "environment": {
    "engine_version": "phrust-0.0.0",
    "rust_target_triple": "aarch64-apple-darwin",
    "opt_flags": ["--opt-level=0"],
    "feature_flags": {}
  },
  "measurements": []
}
```

Validation:

```bash
nix develop -c cargo test -p php_perf
```

## Phase Timing Sidecars

`php-vm run` and `php-vm compile` accept an out-of-band timing sink:

```bash
nix develop -c target/debug/php-vm run --timings-json target/performance/timings/run.json path/to/file.php
nix develop -c target/debug/php-vm compile --timings-json target/performance/timings/compile.json path/to/file.php
```

`PHRUST_TIMINGS_JSON` provides the same sink for both commands. The explicit
`--timings-json` flag wins over the environment variable. Timing JSON is never
written to PHP stdout.

The sidecar schema is:

```json
{
  "schema_version": 1,
  "command": "run",
  "path": "path/to/file.php",
  "total_internal_ms": 1.0,
  "phases": {
    "source_read_ms": 0.1,
    "frontend_analyze_ms": 0.2,
    "ir_lower_ms": 0.3,
    "execute_ms": 0.4
  },
  "counts": {
    "source_bytes": 123,
    "instructions_or_ir_ops": 45
  },
  "flags": {
    "opt_level": "0"
  }
}
```

The CLI emits sorted, pretty JSON with a trailing newline. Phase names are
stable, but unreached phases are omitted instead of reported as fake zeroes. For
example, cache load/store phases only appear when the bytecode cache path is
used, `optimizer_ms` appears only when optimization is requested, and
`bytecode_lower_ms`, `bytecode_layout_ms`, and `superinstruction_select_ms`
appear only on bytecode paths that expose those sub-steps. `ir_lower_ms`
includes the lowering-owned verification performed by the IR layer;
`ir_verify_ms` is reserved for the explicit post-optimization verification pass.
`timings_write_ms` is measured by writing the sidecar once, recording the write
cost, and rewriting the final report.

## Benchmark Runner

Work item adds `scripts/performance/bench_matrix.py`. The runner discovers only
top-level `*.php` files in `tests/fixtures/performance/perf_smoke`, reads the matching
`*.php.out` expected output, and invokes engines with argument vectors rather
than interpolated shell commands. Fixture names are therefore not shell input.

The runner normalizes each process environment with:

- `TZ=UTC`
- `LC_ALL=C`
- `LANG=C`
- `TMPDIR`, `TMP`, and `TEMP` under `target/performance/tmp`
- deterministic seed environment variables for future generated fixtures

`just benchmark-smoke` builds `php-vm`, runs the Rust engine over the smoke
corpus, and writes `target/performance/benchmark-smoke.json`. If `REFERENCE_PHP`
is set, or the pinned `third_party/php-src/sapi/cli/php` binary exists, the same
fixtures are also run as a separate `reference-php` engine in the same report.
Reference PHP absence is recorded as a skip reason in report environment
metadata and does not fail the smoke.

Wall-clock values in these reports are advisory smoke measurements only. A run
fails if an engine exits non-zero or if the last measured output differs from
the fixture's expected output.

Rust VM benchmark rows also write timing sidecars under
`target/performance/timings/bench/` and embed the latest parsed sidecar as
`phase_timings`. Derived metrics include `external_wall_ms`,
`internal_total_ms`, `startup_external_ms`, `compile_total_ms`, `execute_ms`,
`compile_share_percent`, and `execute_share_percent`. Missing or malformed
sidecars are preserved as `timing_warnings`.

## VM/Runtime Counters

Work item adds optional VM counters behind `VmOptions::collect_counters` and
the CLI flag:

```bash
nix develop -c target/debug/php-vm run --counters-json target/performance/counters.json path/to/file.php
```

Counters are off by default and are returned out-of-band through `VmResult` or a
JSON file. They are never appended to PHP stdout. The current counter set
records executed instructions, stable opcode-family counts, function calls,
method calls, array-dimension fetches, property fetches/accesses, `instanceof`
type checks, include/require instructions, autoload attempts, string concats,
runtime fast-path hits/misses, quickening events, inline-cache events, dispatch
cache events, and JIT compile/execute/fallback counters.

`bench_matrix.py` enables Rust VM counters by default and embeds the parsed
counter JSON under `vm_counters` for Rust measurements. Reference-PHP
measurements remain separate and do not carry VM counters.

Validation:

```bash
nix develop -c just benchmark-smoke
nix develop -c jq . target/performance/*.json
```

## Decision Baseline

`just perf-decision-baseline` builds the VM, runs a local benchmark smoke and
app-flow smoke, and writes:

```text
target/performance/decision/benchmark-summary.json
target/performance/decision/app-flow-summary.json
target/performance/decision/app-flows/matrix.json
target/performance/decision/startup-summary.json
target/performance/decision/summary.json
target/performance/decision/summary.md
```

The decision summary ranks startup, compile, and execute costs using the phase
timing sidecars. It is prioritization evidence only; correctness still comes
from the benchmark/app-flow pass status and the broader performance gates.
The default local knobs are `PHRUST_PERF_DECISION_ITERATIONS=10`,
`PHRUST_PERF_DECISION_WARMUPS=3`, `PHRUST_PERF_DECISION_SCALE=2`, and
`PHRUST_PERF_DECISION_TIMEOUT=${PHRUST_APP_FLOW_TIMEOUT:-30.0}`.

`just startup-matrix` runs the focused startup attribution workflow. It measures
debug and release `php-vm --help`, plus baseline and fast empty-script runs,
after building the binaries outside the measured loop. The report includes
`external_wall_ms`, `internal_total_ms` where a timing sidecar exists,
`startup_external_ms`, `binary_size_bytes`, and `profile=debug|release`.

The managed fast profile also skips request-local quickening/tiering setup for
tiny IR units with eight or fewer IR instructions. This is a fixed-cost
execution startup optimization for scripts that are too small to amortize
adaptive metadata. The VM records `adaptive_tiny_unit_setup_skips` when the
policy fires. Baseline mode is unchanged, and larger fast-profile units keep
quickening, inline-cache, and tiering coverage active.

## Optional Local Profiling

Maintainers can use the opt-in recipes in
`docs/performance/profiling-workflow.md` for VM dispatch, array-heavy, call-heavy,
and Composer-like local profiling. These recipes are intentionally outside
standard gates and write any profiler artifacts under `target/performance/profiles/`.

## Optional Release Profiles

The LTO/PGO plan in `docs/performance/release-build-profile.md` is experimental.
It keeps the default debug/dev workflow unchanged and requires comparable
before/after benchmark JSON before making any release-build claim.

## Framework Micro-Smokes

Work item adds offline framework-like smokes in
`tests/fixtures/performance/framework_smoke/`:

- router dispatch;
- Composer/autoload-like lookup;
- DI-container lookup;
- DTO hydration;
- attribute/reflection warm path;
- template-like string output;
- JSON/API-like response generation;
- object property and method loops;
- packed and mixed array traversal.

Run them with:

```bash
nix develop -c just framework-smoke
nix develop -c just perf-report
```

The smoke compares opt-off against opt-on (`--opt-level=2`, quickening on,
inline caches on), checks stdout/stderr/exit-status parity, writes
`target/performance/framework-smoke/summary.{json,md}`. `perf-report` includes
that summary when present. The fixtures are local and do not use Packagist or
vendored framework repositories. They collect counters, not wall-clock timings;
any timing derived from this corpus is local and advisory.
