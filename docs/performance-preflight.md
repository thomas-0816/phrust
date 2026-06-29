# Performance Preflight

Performance work is limited to correctness-preserving performance
infrastructure for the PHP 8.5.7 Rust engine. Optimizations may change internal
representation, caching, dispatch, and measurement, but they must preserve
PHP-visible output, diagnostics, exit status, and side-effect order.

## Current Workspace Surface

- Target PHP reference: PHP 8.5.7 / `php-8.5.7`.
- Dev environment: Nix flakes; validation commands run with
  `nix develop -c ...`.
- Engine pipeline:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm -> php_vm_cli
```

- Performance-owned crates and tools include `php_optimizer`,
  `php_bytecode_cache`, `php_jit`, performance scripts under
  `scripts/performance/`, and performance fixtures under
  `tests/fixtures/performance/`.
- Supporting layers include `php_ir`, `php_runtime`, `php_std`,
  `php_testkit`, `php_vm`, and `php_vm_cli`.

## Validation Surface

Use the aggregate performance gate for performance-related changes:

```bash
nix develop -c just verify-performance
```

The gate includes:

- `performance-tests`
- `performance-regression`
- `cache-roundtrip`
- `optimizer-diff`
- `quickening-smoke`
- `inline-cache-smoke`
- `callgrind-smoke`
- `jit-smoke`
- `safety-audit-smoke`
- `benchmark-smoke`
- `framework-smoke`
- `hotpath-inventory`
- `perf-report`

Long benchmark suites remain opt-in and are not default pull-request gates:

```bash
nix develop -c just benchmark-suite
```

## Regression Baseline

`scripts/performance_regression_smoke.sh` checks that foundation, lexer,
frontend, runtime, standard-library, and performance validation surfaces remain
discoverable. It does not replace the owned layer gates. Run the owning gate
directly when changing that layer:

```bash
nix develop -c just verify-foundation
nix develop -c just verify-lexer
nix develop -c just verify-frontend
nix develop -c just verify-runtime
nix develop -c just verify-stdlib
nix develop -c just verify-performance
```

## Benchmark and Profiling Surface

Performance measurement uses deterministic smoke fixtures, JSON reports,
baseline comparison, and generated summaries under `target/performance/`.
Committed docs summarize current methodology and results; raw run artifacts
must not be committed.

Current performance entry points include:

- `just benchmark-smoke`
- `just framework-smoke`
- `just perf-flag-matrix`
- `just fastest-engine-matrix`
- `just fastest-hotpath-report`
- `just perf-report`
- `just rust-hotpath-bench`
- `just callgrind-smoke`

## Risk Model

- Correctness risk: optimized paths must not change PHP-visible behavior.
- Invalidation risk: quickening, inline caches, and bytecode cache entries must
  be invalidated by relevant source, configuration, include, autoload, function,
  class, method, and property changes.
- Measurement risk: smoke-level measurements are useful for regressions and
  prioritization but not sufficient for broad speed claims.
- Cache risk: bytecode-cache input is untrusted local data and must be
  fingerprinted, versioned, verified, and safely ignored on corruption.
- JIT risk: JIT behavior is experimental, default-off, feature-gated, and
  subject to explicit safety and fallback checks.

## Related Docs

- `docs/performance-methodology.md`
- `docs/performance-runtime.md`
- `docs/performance-results.md`
- `docs/performance-known-gaps.md`
- `docs/performance-bytecode-cache.md`
- `docs/performance-quickening-inline-caches.md`
- `docs/performance-jit-experiment.md`
