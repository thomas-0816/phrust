# Performance documentation

This directory owns performance methodology, optimization contracts, benchmark
fixtures, and performance gates for the mandatory native engine.

Stable contracts:

- [Native execution architecture](../adr/0017-native-execution-architecture.md)
- [Methodology](methodology.md)
- [Native telemetry families](counter-families.md)
- [Optimization gates](optimization-gates.md)
- [Benchmark corpus](benchmark-corpus.md)
- [CI policy](ci-policy.md)

Implementation and profiling notes:

- [Application flows](app-flows.md)
- [Array fast paths](array-fast-paths.md)
- [Array shapes](array-shapes.md)
- [Autoload lookup cache](autoload-lookup-cache.md)
- [Builtin intrinsics](builtin-intrinsics.md)
- [Class-relation caches](class-relation-caches.md)
- [Exit policy](exit-policy.md)
- [Include-path cache](include-path-cache.md)
- [Internal-function dispatch cache](internal-function-dispatch-cache.md)
- [Local-slot layout](local-slot-layout.md)
- [Numeric-string cache](numeric-string-cache.md)
- [Optimizer passes](optimizer-passes.md)
- [Performance preflight](preflight.md)
- [Profiling workflow](profiling-workflow.md)
- [Performance ratchet](ratchet.md)
- [Native compile-record cache](native-compile-cache.md)
- [Executable native SSA and value lifetimes](native-ssa-lifetimes.md)
- [Region profiling](region-profiling.md)
- [Release build profile](release-build-profile.md)
- [Shared-cache research](research-shared-cache.md)
- [SIMD byte kernels](simd-byte-kernels.md)
- [Type-check fast paths](typecheck-fast-paths.md)
- [WordPress warm-runtime profile, 2026-07-19](wordpress-warm-runtime-profile-2026-07-19.md)

Generated counters, JSON, profiles, and benchmark reports stay under
`target/performance/`. Every correctness comparison uses the `baseline` and
`default` policies of the same Cranelift compiler or the external PHP 8.5.7
oracle. Performance tooling must not introduce a second execution backend.
