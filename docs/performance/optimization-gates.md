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
| Baseline-native guarded subset (scalar arithmetic/branches over declared locals) | `SUBSET_ALLOWED` | The executable-memory/W^X, helper-registry, ABI-hash, live-state, and side-exit/deopt prerequisites in `docs/adr/0787-fast-baseline-native-tier-prerequisites.md` are now owned and tested (`crates/php_jit/src/code_memory.rs`, `helpers.rs`, `abi.rs`, `region_ir/osr.rs`). A narrow guarded native subset over verified dense bytecode is allowed when it marshals live slots in/out through the VM, guards every specialized shape, records a typed side-exit reason and interpreter resume target on any unproven shape, and stays default-off behind the JIT feature gate. Reference/COW/foreach/exception/generator/fiber shapes are rejected (not executed), matching the OSR motion policy. |
| Broad / generic machine-code execution of whole functions | `HARD_BLOCK` | Broad generic native execution of arbitrary functions stays blocked until the remaining ADR 0787 items are owned (frame-model completion, source maps, foreach/exception/`finally`/destructor materialization, generator/fiber snapshots, diagnostics/output-byte proof) and PHPT/reference parity gates pass. |
| Broad JIT / OSR / mid-region resume | `HARD_BLOCK` | Blocked until exact live-state and resume state exist; see `docs/performance/deopt-live-state-osr-metadata.md`. The existing narrow guarded Cranelift regions stay bounded by their current policy. |
| Dense object/method/property execution | `SUBSET_ALLOWED` | Broad generic dense object semantics stay blocked. Narrow dense subsets (declared-slot property fetch/assign, IC-resolved method dispatch, guarded callable dispatch) are allowed when they call the existing rich-path helpers and record fallback reasons. |
| By-reference / reference / COW optimization | `SUBSET_ALLOWED` | Optimizing through escaped or unknown references stays blocked (`docs/performance/reference-aliasing-deopt.md`). No-reference paths and proven local/location-based interpreter paths are allowed. By-ref argument location encoding that avoids materializing caller arguments as value registers is explicitly allowed, provided binding reuses the generic reference-cell helpers and fallback counters remain intact. |
| Builtin intrinsics/stubs | `EVIDENCE_GATE` | Broad arginfo-generated stub generation stays gated. Exact per-builtin stubs are allowed when named args, by-ref, coercion, warnings/errors, and reflection behavior are covered by differential fixtures and fallback-reason counters. |
| Compiled-unit cache / warm-worker cache | `SUBSET_ALLOWED` | Process-local or disk-backed compiled-unit caching with strict fingerprint invalidation and correctness gates is allowed (the CLI bytecode cache and the server compiled-script cache follow this policy). Warm-cache results must not be presented as the cold CLI fairness matrix. |
| Optimizer passes over dense bytecode | `EVIDENCE_GATE` | Verifier-bracketed passes with per-pass attempted/applied/skipped/rollback counters, A/B disable switches, and parity fixtures. |

Wall-clock-only evidence never satisfies a gate; counters, parity fixtures,
and fallback attribution do.
