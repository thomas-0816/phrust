# Cranelift OSR / Trace-JIT Research For Phase 7

Date: 2026-06-24.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document covers Optional 07.CL.D. It evaluates on-stack replacement
(OSR) and trace-JIT ideas for PHP loops. It does not implement OSR, does not
add trace recording, and does not change runtime behavior.

## Recommendation

Do not implement OSR or trace JIT in Phase 7. Keep Cranelift compilation
entry-only, feature-gated, and default-off. Phase 8 should focus on widening
entry-point native subsets only where the current side-exit, blacklist, cache,
and fixture infrastructure can prove correctness. OSR should be reconsidered
no earlier than Phase 9, after the VM has explicit live-state maps, resumable
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
Phase 7 results.

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

The current Phase 7 addendum already has a better-controlled path: compile
narrow function-level candidates, report side exits, blacklist unstable
regions, and keep default execution unchanged.

## PHP Workload Benefit Assessment

| Workload pattern | OSR or trace-JIT upside | Phase 7 risk |
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

## Phase 8/9 Decision

Recommended path:

1. Phase 8: do not pursue OSR as a product feature. Continue entry-point JIT
   expansion only where interpreter fallback and side exits already have stable
   reports.
2. Phase 8 research only: add non-executing metadata for loop headers and live
   locals if it is useful for diagnostics or future planning.
3. Phase 9: reconsider OSR only after deopt snapshots, live-state maps,
   `foreach` iteration state, exception/finally/destructor order, and
   reference-cell identity are represented in VM-owned structures and tested
   independently.
4. Trace JIT: defer until after OSR fundamentals exist. A trace JIT without
   deopt ownership would be a speculative benchmark harness, not a correct PHP
   execution layer.

## Phase 7 Scope Boundary

Phase 7 remains entry-only:

- no mid-loop native entry;
- no trace recorder;
- no trace cache;
- no native frame reconstruction from arbitrary instruction points;
- no implicit widening of loop, call, array, object, or `foreach` semantics.

This boundary prevents scope creep. Future OSR or trace-JIT work must start
with a dedicated ADR, isolated feature flags, focused fixtures, and a report
that proves correctness before claiming performance.
