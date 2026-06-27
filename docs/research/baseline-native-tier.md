# Baseline Native Tier Research

Date: 2026-06-27.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document covers Phase 09.15 of the Performance acceleration plan. It
compares possible default-off baseline native tiers for `phrust` and records
the safety pieces required before any executable prototype can become a product
path. It does not change runtime defaults and does not add executable native
code.

## Recommendation

Keep baseline native-tier work research-only for now. Do not start an
executable copy-and-patch, stencil, or partial-evaluation baseline JIT until the
VM owns deoptimization records, live-state maps, reference/COW identity maps,
and an executable-memory policy that is tested independently of benchmarks.

The near-term acceleration path should stay:

1. Tier 0: compact interpreter and dense bytecode coverage.
2. Tier 1: quickening, inline caches, superinstructions, and runtime fast paths.
3. Tier 2b: default-off Cranelift only for proven hot regions.

The existing `CraneliftNoExecBackend` and Cranelift report tooling are enough
for deterministic non-production native-shape experiments. Adding a second
mock backend only for one textual stencil would duplicate the JIT abstraction
without proving PHP-visible behavior, so Phase 09.15 intentionally remains
docs-only.

## Strategy Comparison

| Strategy | Upside | Required proof before execution | Recommendation |
| --- | --- | --- | --- |
| Copy-and-patch or stencil baseline JIT | Very low compile latency for dense bytecode sequences; can reuse interpreter helper calls and inline small opcode templates. | Stable bytecode ABI, relocation metadata, executable-memory owner, helper ABI hash, deopt records for every helper/status exit, live-state maps, reference/COW maps, and per-platform W^X tests. | Defer. Revisit after dense bytecode covers calls, arrays, properties, foreach, and exceptions with explicit side-exit state. |
| Partial-evaluation-derived baseline JIT | Could specialize the current interpreter loop without hand-writing every template. | Deterministic specialization inputs, source-span preservation, guard synthesis, side-exit records, bailout minimization, compile-cache invalidation, and debug mapping back to IR/bytecode. | Research only. Higher implementation risk than copy-and-patch until the interpreter state model is more explicit. |
| Cranelift-only tiering | Reuses the existing default-off `jit-cranelift` infrastructure, helper registry, ABI hash, counters, guard reports, and diff harnesses. | Broader Big-Win evidence, lower compile overhead, stable side-exit rates, and continued feature-gated/default-off policy. | Continue selectively for packed integer reductions and stable hot loops. Do not make it the baseline native tier. |
| Interpreter plus quickening only | Keeps all execution in the VM, with existing off switches and fixtures. Avoids native-code safety and platform policy work. | Continued A/B fixtures, counters, and regression gates as optimized opcode families expand. | Preferred near-term product path. It gives most correctness leverage while runtime semantics are still expanding. |

## Production Safety Requirements

Executable native baseline work needs all of these pieces before it can run by
default or handle general PHP code:

| Requirement | Minimum acceptable shape |
| --- | --- |
| Executable-memory policy | One VM-owned allocator or backend abstraction with explicit platform support, no ad hoc `mmap` or `mprotect` call sites, and fail-closed behavior on unsupported hosts. |
| W^X / protection transitions | Documented write-then-execute transitions, platform-specific tests, and no simultaneously writable/executable pages in owned code paths. |
| ABI hash | A stable hash covering value layout, frame layout, helper signatures, exit statuses, pointer width, and backend configuration. |
| Helper registry | Versioned helper ids, names, signatures, side effects, diagnostics behavior, and return-status meanings. |
| Deopt and side-exit records | A typed exit reason plus exact VM resume target for guard failures, helper failures, overflow, type mismatch, unsupported locals, exceptions, and bailout. |
| Live-state maps | Per-native-point maps for VM locals, registers, temporaries, return slots, exception state, output side effects, and source spans. |
| References and COW | Identity-preserving representation for reference cells, aliases, COW sharing, and separation points. |
| `foreach` state | Iterator position, key/value state, by-value/by-reference mode, mutation epoch, packed/mixed layout, and resume semantics. |
| `try`/`finally` and exceptions | Native exits must preserve unwind order, finally execution, pending exception state, and destructor order. |
| Generators and fibers | No native entry or resume until suspended VM state and native live state can be represented without losing identity. |
| Diagnostics and output | Stderr/runtime diagnostics, warning order, output buffering, callbacks, and conversion errors must match interpreter order. |

## Prototype Decision

No new Rust prototype is added in Phase 09.15. A no-exec textual backend is
low-risk only if it reuses an existing backend abstraction without implying an
execution policy. The repository already has that shape in the Cranelift
addendum: the backend can emit deterministic diagnostics, CLIF descriptors, ABI
hashes, guard reports, and non-default reports without enabling default native
execution.

A future baseline-native prototype should start with a separate ADR and write
only local artifacts under:

```text
target/performance/baseline-native/
```

The first acceptable prototype should be non-executing and should emit a
deterministic stencil descriptor for one verified dense-bytecode sequence, for
example `load_const`, `binary add`, `echo`, and `return`. It must also emit the
required deopt slots even if they are all marked unsupported.

## Handoff

Baseline native work may proceed to an executable prototype later only when all
of these are true:

- dense bytecode has enough coverage that the baseline tier does not need a
  parallel frontend or string-matching execution path;
- side exits and live-state maps are VM-owned structures with tests;
- reference, COW, foreach, exception, generator, and fiber states have explicit
  unsupported or resume representations;
- executable memory and W^X policy are documented and tested for supported
  platforms;
- the acceleration matrix shows interpreter-side Tier 0/Tier 1 limits that
  cannot be solved more safely with quickening, inline caches, or helper fast
  paths.

Until then, baseline native remains research-only and default execution remains
the interpreter.
