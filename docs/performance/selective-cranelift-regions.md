# Selective Cranelift Region Evaluation

This document records the current selective Cranelift region policy for the
fastest-engine track. It is an evaluation of the existing Cranelift surface on
current `main`, not a default-on JIT promotion.

Cranelift remains behind the non-default `jit-cranelift` Cargo feature and the
explicit runtime `--jit=cranelift` mode. `--jit=off` remains the baseline and
source-of-truth execution path.

## Eligible Region Families

Current region eligibility is intentionally narrow and reportable through
`crates/php_jit/src/eligibility.rs`:

| Family | Status | Evidence |
| --- | --- | --- |
| All-int packed foreach reductions | Keep and expand | `PackedForeachIntSumCandidate`, runtime-owned packed metadata guards, large hot-row fixture, layout and overflow side-exit rows. |
| Counted numeric loops without calls | Keep experimental | Int-only loop and branch fixtures execute with zero helper calls, while loop-body calls fall back with `CL-GAP-LOOP-BODY-CALL`. |
| Stable packed int fetch | Keep experimental | `PackedArrayFetchCandidate` covers required by-value array and int params, helper-owned layout and bounds checks, and packed-fetch exit counters. |
| Exact known `strlen`/`count` calls | Keep experimental only | Current rows are helper-backed and mostly parity/noise. No broader builtin-loop expansion is justified until counters prove helper overhead is lower than interpreter dispatch. |

The current decision matrix in `target/performance/cranelift/results.md` limits
real performance-win language to all-int packed foreach reduction. Other
families remain correctness infrastructure, metadata groundwork, or
experimental rows.

## Rejection And Fallback Policy

Eligibility and VM dispatch keep unsupported PHP semantics on the interpreter
path. Current rejection, fallback, or side-exit coverage includes:

- references and by-reference function or foreach shapes;
- COW, mutation, mixed-layout, numeric-string-key, and reference-bearing array
  ambiguity through runtime-owned packed metadata and array fixtures;
- magic methods, property hooks, dynamic dispatch, subclass guard failures, and
  uninitialized typed-property paths through metadata, property, method, and
  guard fixtures;
- exceptions and callee failures through method-call fallback fixtures;
- generators, closures, methods where not explicitly covered, and returns by
  reference through eligibility shape checks;
- dynamic or arbitrary loop-body calls through deterministic fallback rows;
- conversion-sensitive string cases, including string/int concat and object
  `__toString`, through fallback rows.

Every native path has an interpreter fallback. Side exits are recorded with
stable reasons and compact JSON maps, and repeated unstable regions feed the
process-local blacklist.

## Compile Budget And Cache Policy

The VM avoids cold compiles through the Cranelift tiering policy:

- `--jit-threshold=N` controls the function-entry threshold;
- `--tiering-loop-threshold=N` contributes loop-backedge hotness;
- `--jit-max-functions=N` caps per-request compiled functions;
- `--jit-max-compile-us=N` caps per-request native compile time;
- `--jit-eager` is retained only as a test convenience for rows that must prove
  native execution immediately.

The process-local compile cache is keyed by function id, IR fingerprint,
`JIT_RUNTIME_ABI_HASH`, JIT config hash, and target ISA. Entries also validate
runtime/class layout epoch before reuse and are invalidated on epoch mismatch
or function blacklisting. No persistent native cache is enabled.

## Reports And Counters

The selective Cranelift gate is evaluated through these committed surfaces:

- `target/performance/cranelift/results.md` for feature classification,
  compile/execution split, side-exit rates, default status, and recommendation;
- `docs/performance/cranelift/benchmark-methodology.md` for required row families and
  report contracts;
- `docs/performance/cranelift/known-gaps.md` for implemented Cranelift gaps and
  remaining native-subset boundaries;
- `scripts/performance/cranelift/jit_bench_matrix.py` for schema-version-2
  matrix rows and optional reference-PHP orientation;
- `scripts/performance/cranelift/guard_failure_report.py` for side-exit,
  blacklist, and `keep|specialize|blacklist|unsupported` recommendations.

Relevant counters include compile attempts/successes, code bytes, compile time,
executed regions, helper calls, fast-path hits, packed-fetch exits,
packed-foreach layout and overflow exits, known-call guard exits, property and
method guard exits, side-exit reasons, blacklist reasons, tiering cold/hot/eager
decisions, budget rejections, blacklist rejections, and compile-cache
hit/miss/invalidation counts.

## Candidate Evaluation (incremental tranche)

The evaluated next candidate is extending `PackedForeachIntSumCandidate` to the
other associative all-int packed-foreach reductions — `min`/`max`/`product` — the
narrowest family adjacent to the one already eligible. It shares the same
runtime-owned packed metadata guards, overflow/layout side-exit shape, and
by-value foreach constraints, so its correctness surface is understood.

**Decision: not added this tranche.** The pack's gate requires *positive
work-to-compile evidence* before a native region is added, and no fresh
Cranelift benchmark was run here — the tier is feature-gated (`jit-cranelift`),
its benchmarks are host-noisy, and the committed evidence
(`target/performance/cranelift/results.md`, local only) currently justifies a
real performance-win claim only for the existing all-int packed foreach *sum*
reduction. Adding native codegen for a new reduction without that evidence would
violate the evidence-gate and the reject-by-default discipline, so the candidate
stays interpreter-only.

A future expansion of this candidate must show, on the reduction fixture:

- a work-to-compile ratio above 1 (compile cost amortized by executed work);
- bounded, enumerated side exits (overflow, mixed layout, reference/COW,
  numeric-string keys) each with a fixture;
- zero PHP-visible output/diagnostic/exit-status change vs. the interpreter;
- no default-runtime regression (Cranelift stays optional/non-default).

Until then the eligible set is unchanged and the interpreter remains the
fallback for every rejected or unproven shape.

## Current Decision

The selective Cranelift region gate is closed for current fastest-engine work:

- Cranelift did not become default-on.
- The implementation did not broaden to generic function JIT.
- Region-level eligibility and fixtures cover the packed/numeric families the
  current evidence supports.
- Compile budget, hotness, cache-key, side-exit, and blacklist accounting are
  present and reportable.
- Guard reports already produce recommendations for keep, specialize,
  blacklist, and unsupported rows.

Future Cranelift expansion must start from new counter evidence. The next safe
expansion candidates are narrower packed numeric reductions or other
work-to-compile-positive regions. Broader builtins, object/method inlining,
conversion-sensitive strings, dynamic calls, references, exceptions,
generators, fibers, and OSR remain out of scope until deopt/live-state metadata
and differential fixtures prove interpreter-equivalent behavior.
