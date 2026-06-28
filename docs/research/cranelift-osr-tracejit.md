# Cranelift OSR / Trace-JIT Research For Performance

Date: 2026-06-24.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document covers Optional 07.CL.D. It evaluates on-stack replacement
(OSR) and trace-JIT ideas for PHP loops. It does not implement OSR, does not
add an executable trace cache, and does not change runtime behavior. FPE-28
adds opt-in metadata-only region profile JSON for future trace-shape research;
that report remains advisory and never replaces VM execution.

## Recommendation

Do not implement OSR or trace JIT in Performance. Keep Cranelift compilation
entry-only, feature-gated, and default-off. future runtime should focus on widening
entry-point native subsets only where the current side-exit, blacklist, cache,
and fixture infrastructure can prove correctness. OSR should be reconsidered
no earlier than PHPT runtime, after the VM has explicit live-state maps, resumable
deopt snapshots, loop-header metadata, and request-lifecycle tests for
exceptions, destructors, arrays, and `foreach`.

Trace JIT should remain lower priority than structured entry-point JIT work.
It can be useful for highly dynamic hot loops, but PHP control flow, autoload,
destructors, reference semantics, array shape changes, and exception edges make
implicit trace stitching risky without a stronger deoptimization model.

## What OSR Would Require

| Requirement | Needed design |
| --- | --- |
| Live-state maps | Every OSR entry and exit point needs a precise map from native values to VM frame locals, registers, temporaries, active exception state, and pending return slots. Maps must use bytecode/IR source positions, not machine-code offsets alone. |
| Loop headers | HIR/IR must mark stable loop headers, induction variables, loop-carried locals, break/continue targets, and dominance relationships. Header metadata must distinguish counted loops from `foreach`, `while`, and loops with calls. |
| Deopt snapshots | Native code must be able to materialize interpreter frames at side exits, guard failures, overflow exits, helper failures, and explicit bailouts. Snapshots need value kinds, local initialization state, references, and source spans for diagnostics. |
| Exception/destructor handling | OSR cannot skip `finally`, pending exceptions, object destructors, generator/fiber state, or request-shutdown semantics. Any deopt path must preserve unwind order and must not run destructors twice or suppress them. |
| `foreach` state | PHP array iteration needs current key/value state, by-value versus by-reference mode, mutation behavior, packed/mixed layout transitions, and interaction with references. A native loop must be able to re-enter the interpreter at the exact iteration point. |
| Reference cells | Locals and array elements may be references. OSR maps must preserve identity and aliasing, not only scalar values. |
| Calls and autoload | Calls inside hot loops can invoke user code, trigger autoload, mutate globals/classes, throw, or change inline-cache state. OSR needs invalidation and deopt hooks around those calls. |
| Debug/profiling evidence | The report matrix must separate compile time, OSR entry time, side-exit cost, and steady-state loop time, with no speedup threshold in default CI. |

Without these pieces, an OSR implementation would either silently produce
incorrect PHP behavior or fall back so often that it would obscure the current
Performance results.

## Trace-JIT Requirements

A trace JIT would add additional machinery beyond OSR:

- deterministic trace recording that does not change stdout, stderr, or
  diagnostics;
- guards for value type, array layout, class layout, helper return status,
  reference identity, and call target stability;
- side-trace linking rules with megamorphic bailout thresholds;
- trace invalidation on class layout, function table, helper ABI, and config
  hash changes;
- a trace cache distinct from the current function compile cache;
- reproducible minimizer output for divergent traces;
- tooling to map trace instructions back to source spans and IR nodes.

FPE-28 now provides a first metadata-only input for this research through
`php-vm run --region-profile-json <path>`. It records bounded framework-like
region summaries, stable hashed callsites, numeric function/method IDs, IC and
shape metadata, include/autoload events, and conservative rejection reasons.
It still does not record executable traces, link side traces, install native
guards, or own deopt/live-state resume.

The current Performance addendum already has a better-controlled path: compile
narrow function-level candidates, report side exits, blacklist unstable
regions, and keep default execution unchanged.

## PHP Workload Benefit Assessment

| Workload pattern | OSR or trace-JIT upside | Performance risk |
| --- | --- | --- |
| Numeric counted loops | Potentially useful once loop bodies are wider than the current entry-only subset. | Current entry-point native loops already cover the narrow int cases; OSR adds complexity before broader loop bodies exist. |
| Packed-array reductions | Potentially useful for long loops when array shape stays stable. | `foreach` mutation, by-reference iteration, mixed elements, overflow, and helper failures need exact deopt state. |
| Framework dispatch loops | Low to mixed. Many hot paths are call-heavy and object-heavy rather than pure loop arithmetic. | Calls, autoload, exceptions, and object layout changes make trace invalidation central. |
| Template/string loops | Mixed. Concatenation can be hot, but allocation and helper behavior dominate. | Native traces would still need allocator/string helper side exits and destructor safety. |
| DTO hydration/service loops | Potentially useful after monomorphic property/method paths mature. | Polymorphism and magic methods can quickly become megamorphic; trace invalidation must be proven first. |

The likely near-term win is not OSR itself. It is better entry-point coverage
for the existing Big-Win families: packed arrays, property reads, direct method
calls, known calls, and string concatenation. Those paths can use the current
diff harness and guard report without inventing mid-frame resume.

## future runtime Decision

Recommended path:

1. future runtime: do not pursue OSR as a product feature. Continue entry-point JIT
   expansion only where interpreter fallback and side exits already have stable
   reports.
2. future runtime research only: build on the non-executing VM-owned metadata in
   `php_vm::deopt` and `docs/performance-deopt-live-state-osr-metadata.md` for
   dense resume points, live-state slots, foreach markers, and explicit
   rejection reasons.
3. PHPT runtime: reconsider OSR only after deopt snapshots, live-state maps,
   `foreach` iteration state, exception/finally/destructor order, and
   reference-cell identity are represented in VM-owned structures and tested
   independently.
4. Trace JIT: defer until after OSR fundamentals exist. A trace JIT without
   deopt ownership would be a speculative benchmark harness, not a correct PHP
   execution layer.

## Performance Scope Boundary

Performance remains entry-only:

- no mid-loop native entry;
- no executable trace recorder;
- no trace cache;
- no native frame reconstruction from arbitrary instruction points;
- no implicit widening of loop, call, array, object, or `foreach` semantics.

This boundary prevents scope creep. Future OSR or trace-JIT work must start
with a dedicated ADR, isolated feature flags, focused fixtures, and a report
that proves correctness before claiming performance.
