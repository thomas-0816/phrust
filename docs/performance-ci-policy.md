# Performance CI Policy

Performance CI uses the same Nix entrypoints as local development. The required
default workflow job is `.github/workflows/ci.yml` and the required
default command is:

```bash
nix develop -c just verify-performance
```

`verify-performance` is the source of truth for required default Performance correctness
and smoke coverage. It runs workspace tests, regression fixtures, the full
performance-flag A/B matrix, bytecode-cache roundtrip checks, optimizer
differential checks, quickening smoke, inline-cache smoke, skip-safe Callgrind
smoke, default-off JIT smoke, safety audit smoke, benchmark smoke, framework
smoke, release benchmark smoke, hot-path inventory, and perf-report generation.
The workspace test step runs with `RUST_MIN_STACK` defaulting to `8388608`
bytes, overridable with `PHRUST_RUST_MIN_STACK`, so recursive VM tests use a
deterministic stack budget in local and CI `just` gates. The repo-local Cargo
config also applies the same default stack to direct Unix `cargo test` runs.

The focused commands remain available for local bisection and CI log triage:

```bash
nix develop -c just cache-roundtrip
nix develop -c just optimizer-diff
nix develop -c just quickening-smoke
nix develop -c just inline-cache-smoke
nix develop -c just jit-smoke
nix develop -c just release-benchmark-smoke
```

Optional production-profile experiments are available but are not hard CI
requirements:

```bash
nix develop -c just pgo-benchmark-smoke
nix develop -c just bolt-benchmark-smoke
```

The optional recipes must write a machine-readable skip report when host tools,
platform support, opt-in environment variables, or BOLT perf data are missing.
They must not turn local host timings into pull-request speed budgets.

The Cranelift addendum is feature-gated and runs as a separate optional CI job:

```bash
nix develop -c just verify-cranelift
```

This job compiles with `--features jit-cranelift`, runs the Cranelift smoke,
diff, bench-smoke, consolidated report, and guard-report gates, and uploads
`target/performance/cranelift/**/*.json`, `*.md`, and `*.txt` as diagnostic
artifacts. The default `verify-performance` job intentionally does not enable the
Cranelift feature.

The required workflow also runs:

```bash
nix flake check
```

The flake check must remain lightweight. Optional profiling tools, optional
benchmark-only crates, and feature-gated JIT dependencies must not become hard
flake checks. Unsupported JIT/native-code configurations must fail closed,
fallback, or skip with an explicit reason; they must not make pull-request CI
architecture-specific.

Cranelift platform support is probed by:

```bash
scripts/performance/cranelift/platform_check.py --out target/performance/cranelift/platform.json
```

The JSON status is machine-readable:

```json
{
  "schema_version": 1,
  "status": "pass",
  "reason": "host triple is supported for Performance Cranelift smoke gates",
  "host_triple": "x86_64-unknown-linux-gnu"
}
```

Unsupported platforms write the same file with `"status": "skip"` and a stable
`reason`; the public Cranelift just targets then exit successfully after the
skip so optional feature coverage does not make unrelated CI hosts fail.

Long benchmark jobs are not required for every pull request. The same workflow
has an optional benchmark job that runs only on the weekly schedule or when a
maintainer starts `workflow_dispatch` with `run_long_benchmarks=true`:

```bash
nix develop -c just benchmark-suite
nix develop -c just perf-report
```

Benchmark output is uploaded as CI artifacts from `target/performance` and
`target/criterion`. These artifacts are diagnostic evidence for the CI host, not
portable performance budgets.

Host-specific Cranelift reports are also generated only under
`target/performance/cranelift/` and must not be committed.

CI tests must not require secrets, network access from test code, a vendored
`php-src`, or a prebuilt reference PHP binary. Reference-dependent checks keep
the existing policy: they skip clearly when no reference binary is configured
and are strict when `REFERENCE_PHP` is explicitly set.
