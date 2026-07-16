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
  `php_jit`, performance scripts under
  `scripts/performance/`, and performance fixtures under
  `tests/fixtures/performance/`.
- Supporting layers include `php_ir`, `php_runtime`, `php_std`,
  `php_testkit`, `php_vm`, and `php_vm_cli`.

## Validation Surface

Use the aggregate performance gate for performance-related changes:

```bash
nix develop -c just verify-performance
```

The gate composition (and the split into `verify-performance` and
`verify-performance-extended`) is documented once, in
[`ci-policy.md`](ci-policy.md); the `justfile` recipes are the executable
source of truth.

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
- `just default-profile-smoke`
- `just native-smoke`
- `just perf-report`
- `just rust-hotpath-bench`
- `just callgrind-smoke`

## Risk Model

- Correctness risk: optimized paths must not change PHP-visible behavior.
- Invalidation risk: native code, inline caches, and cache entries must
  be invalidated by relevant source, configuration, include, autoload, function,
  class, method, and property changes.
- Measurement risk: smoke-level measurements are useful for regressions and
  prioritization but not sufficient for broad speed claims.
- Cache risk: persistent PNA2 native artifacts are untrusted local data and
  must be fingerprinted, versioned, verified, and safely ignored on corruption.
- Native-code risk: generated code is subject to W^X, ABI, cache-validation,
  transition, and safepoint checks.

## Related Docs

- `docs/performance/methodology.md`
- `docs/adr/0017-native-execution-architecture.md`
- `docs/reference/performance-status.md`
- `docs/performance/known-gaps.md`
- `docs/performance/native-compile-cache.md`
- `docs/performance/counter-families.md`
