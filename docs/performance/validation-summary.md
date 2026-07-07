# Performance Validation Summary

Reference target: PHP 8.5.7 (`php-8.5.7`).

Performance uses the existing engine pipeline:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm -> php_vm_cli
```

No second lexer, parser, AST, semantic frontend, runtime executor, or
source-string execution path was introduced. Performance features are
allowed to change internal representation, caching, dispatch, and measurement,
but must not change PHP-visible stdout, exit status, diagnostics, or side-effect
order.

## Required Gates

Run these before handing off performance changes:

```bash
nix develop -c just verify-performance
nix develop -c just perf-report
```

`verify-performance` expands to:

```bash
nix develop -c just performance-tests
nix develop -c just performance-regression
nix develop -c just cache-roundtrip
nix develop -c just optimizer-diff
nix develop -c just quickening-smoke
nix develop -c just inline-cache-smoke
nix develop -c just callgrind-smoke
nix develop -c just jit-smoke
nix develop -c just safety-audit-smoke
nix develop -c just benchmark-smoke
nix develop -c just framework-smoke
nix develop -c just hotpath-inventory
nix develop -c just perf-report
```

The required CI workflow also runs:

```bash
nix flake check
nix develop -c just verify-performance
```

Long benchmark suites remain optional and are not pull-request gates:

```bash
nix develop -c just benchmark-suite
```

## Evidence Map

| Area | Status | Evidence |
| --- | --- | --- |
| Layer scope and principles | Implemented as performance-only internal layers with behavior preservation requirements. | `docs/adr/0014-performance-scope.md`, `docs/performance/runtime.md` |
| Nix/devshell performance tooling | Implemented with cache/linker optimization where supported and skip-safe optional tooling. | `flake.nix`, `docs/performance/methodology.md` |
| Performance command gates | Implemented through concrete `just` recipes; no placeholder Performance gate remains. | `justfile`, `docs/performance/methodology.md`, `docs/performance/runtime.md`, `docs/performance/known-gaps.md` |
| Benchmark corpus and runner | Implemented deterministic smoke corpus, JSON runner, baseline/compare/report flow, and hot-path inventory. | `tests/fixtures/performance/perf_smoke/`, `scripts/performance/bench_matrix.py`, `scripts/performance/compare_perf_json.py`, `scripts/performance/perf_report.py`, `docs/performance/benchmark-corpus.md`, `docs/reference/performance-status.md` |
| Bytecode cache | Implemented local disk cache artifacts with fingerprints, target/version/config metadata, verified IR payloads, corrupt-cache fallback, CLI read/write modes, and path-component hardening. | `crates/php_bytecode_cache`, `crates/php_vm_cli/src/main.rs`, `docs/adr/0015-bytecode-cache-format.md`, `docs/performance/bytecode-cache.md`, `just cache-roundtrip` |
| Optimizer passes | Implemented pass framework, opt levels 0/1/2, safe constant folding, peepholes, branch simplification, CFG verifier checks, and differential output comparison. | `crates/php_optimizer`, `docs/performance/optimizer-passes.md`, `just optimizer-diff` |
| IR/bytecode invariants | Extended verifier coverage protects optimizer and cache boundaries. | `crates/php_ir/src/verify.rs`, `docs/performance/ir-verifier.md`, `just ir-verify` |
| Quickening | Implemented request-local quickening framework and concrete specializations for int add, string concat, and packed-array dimension fetch with guard/fallback counters. | `crates/php_vm/src/quickening.rs`, `docs/performance/quickening-inline-caches.md`, `just quickening-smoke` |
| Inline caches | Implemented monomorphic function, class/static, include-path, and autoload/class-lookup caches plus fixed-size method/property polymorphic caches with epoch invalidation, megamorphic fallback, and stats. | `crates/php_vm/src/inline_cache.rs`, `docs/performance/quickening-inline-caches.md`, `docs/performance/include-path-cache.md`, `docs/performance/autoload-lookup-cache.md`, `just inline-cache-smoke`, `just polymorphic-inline-cache-smoke` |
| Runtime fast paths | Implemented local-slot/frame reuse, packed-array, numeric-string, typecheck/prologue, internal-dispatch, `count(array)`, and output-buffer fast-path coverage with counters. | `docs/performance/local-slot-layout.md`, `docs/performance/array-fast-paths.md`, `docs/performance/numeric-string-cache.md`, `docs/performance/typecheck-fast-paths.md`, `docs/performance/internal-function-dispatch-cache.md`, `docs/performance/output-buffer-fast-paths.md` |
| Deopt/fallback protocol | Implemented unified fallback/counter surface for optimized paths. | `crates/php_vm/src/fallback.rs`, `docs/performance/quickening-inline-caches.md` |
| Stress regressions | Implemented regression fixtures for exceptions, destructors, generators, fibers, references, COW, and autoload invalidation around optimized paths. | `tests/fixtures/performance/regressions/`, `scripts/performance/regression_smoke.sh`, `docs/performance/regressions.md` |
| Full performance-flag A/B matrix | Implemented baseline versus opt1, opt2, quickening, inline caches, bytecode cache, and all-non-JIT optimization combinations across Performance and selected Runtime semantics fixtures. | `scripts/performance/perf_flag_matrix.py`, `just perf-flag-matrix` |
| Optional instruction-count smoke | Implemented skip-safe Callgrind smoke on Linux with explicit skip reasons elsewhere. | `scripts/performance/callgrind_smoke.sh`, `just callgrind-smoke` |
| Rust hot-path benchmarks | Implemented optional Criterion benchmark crate excluded from the main workspace. | `crates/php_bench`, `just rust-hotpath-bench` |
| JIT experiment | Implemented default-off API, eligibility analyzer, ABI handle model, optional Cranelift lowering, guarded native-entry experiments through the VM tiering path, and fallback counters. It is not production native JIT. | `crates/php_jit`, `docs/adr/0017-cranelift-jit-experiment.md`, `docs/performance/jit-experiment.md`, `just jit-smoke` |
| Executable-memory boundary | Documented Cranelift's JIT memory provider for Cranelift-generated entries, `php_jit::code_memory` for repository-emitted experiments, and the remaining production/native-cache lifecycle blockers. | `docs/adr/0018-cranelift-memory-safety.md`, `docs/adr/0019-fast-baseline-native-tier-prerequisites.md`, `docs/performance/safety-audit.md` |
| Tiering | Implemented interpreter to quickened to JIT policy counters and CLI stats output with default-safe thresholds. | `crates/php_vm/src/tiering.rs`, `docs/performance/quickening-inline-caches.md` |
| Safety audit | Implemented cache/JIT/adaptive-surface unsafe scan, corrupt-cache tests, path traversal hardening, Miri skip policy, and executable-memory status documentation. | `docs/performance/safety-audit.md`, `just safety-audit-smoke` |
| CI/Nix hardening | Implemented Performance workflow with required verify job, manual/scheduled long benchmark job, flake check, report artifacts, and skip policy. | `.github/workflows/ci.yml`, `docs/performance/ci-policy.md` |
| Optional profiling | Implemented maintainer-only recipes that skip by default and write profiler artifacts under `target/performance/profiles/`. | `docs/performance/profiling-workflow.md`, `scripts/performance/profile_smoke.sh` |
| Optional release profiles | Documented LTO/PGO experiments without changing default build profiles. | `docs/performance/release-build-profile.md`, `scripts/performance/release_profile_plan.sh` |
| Shared-cache research | Compared disk cache, mmap, process-local cache, and future shared memory with security/invalidation risks. | `docs/performance/research-shared-cache.md` |
| Framework micro-smokes | Implemented offline router, Composer/autoload-like lookup, DI-container, DTO hydration, attribute/reflection, template-output, JSON/API-like output, object property/method loop, and packed/mixed array traversal smokes with opt-off/on counter comparison, generated corpus summary, `verify-performance`, and perf-report integration. | `tests/fixtures/performance/framework_smoke/`, `scripts/performance/framework_micro_smoke.py`, `docs/performance/framework-corpus.md`, `just framework-smoke`, `just verify-performance`, `just perf-report` |
| Known gaps | Current and explicit; no performance claim depends on unstated gaps. | `docs/performance/known-gaps.md` |

## Current Known Gaps

The authoritative catalog is `docs/performance/known-gaps.md`. The final Performance
carryovers are:

- calibrated performance budgets and optimized-flag benchmark suites;
- broader hot-path corpus representativeness;
- more granular standard-library call counters;
- complete bytecode-cache dependency invalidation for dynamic include paths,
  symlinks, failed include diagnostics, Composer autoload metadata, shared
  memory, preload, and production SAPI lifecycle;
- production native JIT execution, executable-memory ownership, W^X, and native
  entry/exit ABI proof.

These are not silent failures. They are explicit known gaps with owning docs and
validation checks.

## Runtime and Deployment Boundaries

The following areas remain outside the current performance contract:

- production SAPI/FPM/daemon lifecycle, including request reset, worker
  recycling, config reloads, and cache lifetime ownership;
- persistent shared bytecode cache, preload semantics, and dependency
  invalidation for includes, Composer autoload metadata, generated class maps,
  symlinks, and working-directory changes;
- Zend ABI and extension strategy boundaries, including which extensions are
  native Rust implementations versus ABI-compatible bridges;
- calibrated performance budgets in a stable CI environment, ideally with
  Linux instruction-count budgets before wall-clock budgets;
- broader offline framework performance smokes for router dispatch,
  dependency-injection lookup, attribute/reflection warm paths, and template
  output;
- optional native JIT expansion only after W^X/executable-memory policy,
  native call ABI proof, deopt safety, and crash containment gates exist;
- packaging and distribution profiles for CLI and daemon deployment forms.

## Closure Criteria

Performance changes are acceptable when `verify-performance` and `perf-report`
pass. Any red gate must be classified as a new regression, an
environment/tool skip with explicit output, or an existing known gap before the
change is handed off.
