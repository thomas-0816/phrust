# Performance Documentation

This directory owns performance methodology, optimization contracts, benchmark
fixtures, and performance gate contracts.

## Stable Contracts

- [Methodology](methodology.md): measurement and reporting policy.
- [Runtime optimization contract](runtime.md): behavior-preserving VM and
  runtime optimization rules.
- [Optimization gates](optimization-gates.md): allowed, subset-allowed, and
  blocked optimization classes.
- [Bytecode cache](bytecode-cache.md): cache format, validation, and CLI
  behavior.
- [Known gaps](known-gaps.md): performance gaps that remain intentionally open.
- [Fastest-engine known gaps](fastest-engine-known-gaps.md): the `FPE-GAP-*`
  catalog of remaining fastest-engine deltas and their closure requirements —
  the single authoritative delta ledger (point-in-time projections of it are
  not committed; they drift).

## Optimization Areas

- [Quickening and inline caches](quickening-inline-caches.md)
- [Optimizer passes](optimizer-passes.md)
- [Array fast paths](array-fast-paths.md)
- [Internal function dispatch cache](internal-function-dispatch-cache.md)
- [Output buffer fast paths](output-buffer-fast-paths.md)
- [SIMD byte kernels](simd-byte-kernels.md)

## Local Reports

Generated counters, JSON, profiler captures, benchmark matrices, and local
markdown reports stay under `target/performance/`. The public status summary is
[Performance status](../reference/performance-status.md).

## Native Tier

Cranelift and native-tier documents are grouped under
[cranelift/](cranelift/README.md). The native tier remains experimental and
default-off unless a separate ADR changes that policy.

## Additional References

- [Performance Autoload Lookup Cache](autoload-lookup-cache.md)
- [Performance Benchmark Corpus](benchmark-corpus.md)
- [Dense Bytecode Block Layout](bytecode-layout.md)
- [Class Relation Caches](class-relation-caches.md)
- [App-Flow Overhead Counter Families](counter-families.md)
- [Exit-Counter-Guided Specialization Policy](exit-policy.md)
- [Performance Include Path Cache](include-path-cache.md)
- [Performance Local Slot Layout](local-slot-layout.md)
- [Performance Mode Matrix](mode-matrix.md)
- [Performance Preflight](preflight.md)
- [Performance Optimized-Path Regression Fixtures](regressions.md)
- [Performance Shared Cache Research Spike](research-shared-cache.md)
- [Performance Rule Selection](rule-selection.md)
- [Runtime-memory safety audit](runtime-memory-safety-audit.md)
- [Performance Typecheck Fast Paths](typecheck-fast-paths.md)
- [VM frame-memory safety audit](vm-frame-safety-audit.md)
