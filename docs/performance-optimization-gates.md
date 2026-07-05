# Performance Optimization Gates

Date: 2026-07-05.

Performance documentation in this repository distinguishes three gate classes
so that safety blockers stop only the work they actually block. A doc saying an
area is "blocked" without one of these labels means the broad/generic form of
the work; it does not by itself forbid a narrow, guarded interpreter subset.

```text
HARD_BLOCK:
  Broad execution is forbidden until the listed VM contract exists.

SUBSET_ALLOWED:
  Generic optimization is blocked, but a narrow guarded subset may be
  implemented if it reuses baseline semantic helpers, exposes fallback
  counters, and passes differential/reference fixtures.

EVIDENCE_GATE:
  Implementation is allowed only with before/after counters, parity checks,
  fallback reasons, and committed documentation of the evidence pattern.
```

Every gate class shares the correctness floor: fast paths call the same
semantic helpers as the generic path, fall back with a recorded reason on any
unproven shape, and never change PHP-visible stdout, diagnostics, ordering, or
exit status.

## Current classification

| Area | Gate | Notes |
| --- | --- | --- |
| Baseline-native / broad machine-code execution | `HARD_BLOCK` | Blocked until the prerequisites in `docs/adr/fast-baseline-native-tier-prerequisites.md` exist (executable memory, W^X, code-cache lifecycle, ABI/helper hashes, frame/live-state/deopt metadata, reference/COW/foreach/exception state, generator/fiber policy, diagnostics/output proof, PHPT/reference gates). |
| Broad JIT / OSR / mid-region resume | `HARD_BLOCK` | Blocked until exact live-state and resume state exist; see `docs/performance-deopt-live-state-osr-metadata.md`. The existing narrow guarded Cranelift regions stay bounded by their current policy. |
| Dense object/method/property execution | `SUBSET_ALLOWED` | Broad generic dense object semantics stay blocked. Narrow dense subsets (declared-slot property fetch/assign, IC-resolved method dispatch, guarded callable dispatch) are allowed when they call the existing rich-path helpers and record fallback reasons. |
| By-reference / reference / COW optimization | `SUBSET_ALLOWED` | Optimizing through escaped or unknown references stays blocked (`docs/performance-reference-aliasing-deopt.md`). No-reference paths and proven local/location-based interpreter paths are allowed. By-ref argument location encoding that avoids materializing caller arguments as value registers is explicitly allowed, provided binding reuses the generic reference-cell helpers and fallback counters remain intact. |
| Builtin intrinsics/stubs | `EVIDENCE_GATE` | Broad arginfo-generated stub generation stays gated. Exact per-builtin stubs are allowed when named args, by-ref, coercion, warnings/errors, and reflection behavior are covered by differential fixtures and fallback-reason counters. |
| Compiled-unit cache / warm-worker cache | `SUBSET_ALLOWED` | Process-local or disk-backed compiled-unit caching with strict fingerprint invalidation and correctness gates is allowed (the CLI bytecode cache and the server compiled-script cache follow this policy). Warm-cache results must not be presented as the cold CLI fairness matrix. |
| Optimizer passes over dense bytecode | `EVIDENCE_GATE` | Verifier-bracketed passes with per-pass attempted/applied/skipped/rollback counters, A/B disable switches, and parity fixtures. |

Wall-clock-only evidence never satisfies a gate; counters, parity fixtures,
and fallback attribution do.
