# Performance Status

Performance documentation records methodology, optimization contracts, current
gate behavior, and known gaps. Local timing output and generated summaries are
not committed as public documentation.

## Current Contract

- Performance changes must preserve PHP-visible stdout, stderr, diagnostics,
  exit status, side-effect order, and known-gap behavior.
- Wall-clock timings are advisory unless a dedicated gate records and compares a
  baseline under its own policy.
- Generated reports, counter dumps, profiler captures, and benchmark matrices
  belong under `target/performance/`.
- Native-tier and JIT material remains experimental/default-off unless an ADR
  changes that policy.

## Main Commands

```bash
nix develop -c just verify-performance
nix develop -c just benchmark-smoke
nix develop -c just framework-smoke
nix develop -c just fastest-engine-matrix
nix develop -c just fastest-hotpath-report
nix develop -c just perf-report
```

Use [Performance methodology](../performance/methodology.md),
[optimization gates](../performance/optimization-gates.md), and
[performance known gaps](../performance/known-gaps.md) for the accepted
contracts.
