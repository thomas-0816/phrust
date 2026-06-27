# Performance Acceleration Plan

Date: 2026-06-27.

This is the coordination document for the staged performance acceleration prompt
pack. It builds on the existing Performance layer without changing PHP-visible
semantics or promoting native execution to the default path.

The interpreter remains the source of truth. Optimized paths must be optional,
observable through counters, comparable against the baseline, and able to fall
back to generic VM/runtime behavior.

## Current Engine Layers

The active source pipeline remains:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir -> php_runtime -> php_vm -> php_vm_cli
```

Current ownership:

| Layer | Current role | Acceleration rule |
| --- | --- | --- |
| `php_lexer`, `php_syntax`, `php_ast`, `php_semantics` | Frontend, typed views, declarations, HIR, and diagnostics. | Do not add a second lexer, parser, AST, or semantic frontend for performance work. |
| `php_ir` | Rich register IR, source maps, verifier, lowering boundary, and optimizer input. | Keep rich IR as the verified optimizer/frontend boundary. Dense bytecode must be a VM execution format, not a replacement frontend. |
| `php_optimizer` | Conservative opt levels 0/1/2, pass reports, verifier integration, and safe peepholes/folding. | Only add passes with output, stderr, exit-status, diagnostic, and verifier parity evidence. |
| `php_runtime` | PHP values, arrays, strings, output, references/COW, objects, builtins, and runtime services. | Runtime fast paths must call or preserve existing semantic helpers unless a fixture proves the exact shortcut. |
| `php_vm` | Interpreter dispatch, frames, registers, calls, quickening, inline caches, counters, tiering, fallback, includes, dense-bytecode design/execution subset, and JIT integration. | Tier 0 and Tier 1 live here. Every optimized mode needs an off switch, fallback, and counters. Dense bytecode execution is explicit and default-off. |
| `php_bytecode_cache` | Local disk cache for verified artifacts and corruption fallback. | Cache artifacts are untrusted inputs and cannot decide correctness. |
| `php_jit` | Default-off JIT backend API, optional Cranelift feature, ABI/helper surface, and experimental native subsets. | Native execution remains feature-gated and runtime-off unless a future prompt explicitly changes policy. |
| `php_perf` and `scripts/performance/` | Stable report types, benchmark smokes, flag matrix, hot-path inventory, and performance reports. | Correctness comparison comes before timing. Host wall-clock reports stay advisory. |

## Current Performance Gates

`nix develop -c just help` is the command discovery surface. As of this plan,
the relevant performance gates are:

| Command | Current role |
| --- | --- |
| `just verify-performance` | Aggregate performance gate: workspace performance tests, regressions, flag matrix, benchmark smoke, framework smoke, release benchmark smoke, bytecode cache, optimizer, quickening, inline-cache, Callgrind skip/run, default-off JIT smoke, safety audit, hot-path inventory, and report generation. |
| `just performance-tests` | Workspace tests plus performance script self-tests. |
| `just performance-regression` | Historical regression smoke, optimized flag regression fixtures, flag matrix, and polymorphic IC smoke. |
| `just perf-flag-matrix` | A/B correctness matrix for opt levels, quickening, inline caches, bytecode cache, and non-JIT optimized combinations. |
| `just benchmark-smoke` | Deterministic smoke benchmark over `tests/fixtures/performance/perf_smoke/`, writing JSON under `target/performance/`. |
| `just framework-smoke` | Offline framework-like opt-off/opt-on smoke over router, Composer/autoload-like lookup, DI, DTO hydration, template, JSON/API output, object property/method loop, packed/mixed array traversal, and attribute/reflection fixtures. It is part of `verify-performance` and writes `docs/performance-framework-corpus.md`. |
| `just release-benchmark-smoke` | Builds `php-vm` with the explicit `release` Cargo profile, runs the deterministic performance smoke and framework corpus against the release binary, and writes JSON/Markdown under `target/performance/release/`. It is part of `verify-performance`; timings are advisory. |
| `just acceleration-matrix` | Runs the correctness-first Phase 09.16 matrix over baseline IR, dense bytecode auto/strict subset, superinstructions, optimizer levels, quickening, inline caches, all-non-JIT, release, and optional Cranelift rows. It writes JSON/Markdown under `target/performance/acceleration/` and is part of `verify-performance`. |
| `just pgo-benchmark-smoke` | Optional PGO flow. It skips with a report unless `PHRUST_RUN_PGO=1` is set and `llvm-profdata` is available; enabled runs build an instrumented release binary, train on the performance corpus, merge profile data, rebuild with `-Cprofile-use`, and write reports under `target/performance/release/`. |
| `just bolt-benchmark-smoke` | Optional Linux-only BOLT flow. It skips with a report outside Linux, without BOLT tools, or without `PHRUST_RUN_BOLT=1` plus `PHRUST_BOLT_PERF_DATA`; enabled runs consume perf data, emit an optimized binary under `target/performance/release/`, and run the corpus smoke. |
| `just hotpath-inventory` | Generates `docs/hotpath-inventory.md` from benchmark counter totals. |
| `just cache-roundtrip` | Bytecode-cache fingerprint, verifier, corruption, and CLI cache tests. |
| `just optimizer-diff` | IR verifier plus opt-level output/diagnostic parity fixtures. |
| `just superinstruction-smoke` | Dense bytecode A/B smoke comparing `--superinstructions=off` and `--superinstructions=on`, with counter assertions for selected and executed fused opcodes. |
| `just quickening-smoke` | Quickening off/on A/B smoke and counter assertions. |
| `just inline-cache-smoke` | Inline-cache off/on A/B smoke and counter assertions. |
| `just jit-smoke` | Default-off JIT API, eligibility, ABI, feature, and CLI A/B smoke. |
| `just verify-cranelift` | Optional Cranelift feature-on verification with platform skip support. |
| `just perf-report` | Renders local Markdown and JSON performance reports under `target/performance/`. |

The Phase 09.16 acceleration matrix is now the final cross-layer performance
summary gate. It keeps timing advisory and compares PHP-visible behavior before
reporting counters or wall time.

## Current Cranelift Status

Cranelift is extensive but still experimental:

- `jit-cranelift` is a non-default Cargo feature.
- Runtime native execution is non-default and controlled by explicit VM/CLI JIT
  modes.
- `docs/performance-cranelift-results.md` records the current decision matrix:
  most rows are parity, noisy, helper-dominated, or experimental; the narrow
  all-int packed foreach reduction is the only current keep-and-expand speed
  candidate.
- `docs/performance-cranelift-known-gaps.md` and
  `docs/performance-cranelift-completion-audit.md` record the JIT-specific
  evidence and gaps.
- Cranelift remains Tier 2b: selective native compilation for proven hot
  regions only, after interpreter metadata and runtime fast paths have enough
  evidence.

The acceleration program must not start by making Cranelift default-on.

## Target Architecture

### Tier 0: Compact Baseline Interpreter

Tier 0 is the correctness baseline. It should become more compact and cheaper
to dispatch, but it must preserve the existing interpreter semantics, runtime
helpers, diagnostics, exception behavior, references, COW, destructors,
visibility checks, magic methods, include/autoload order, generators, fibers,
and output buffering.

Near-term work:

- introduce a dense executable bytecode representation as a VM-only format;
- verify dense bytecode before execution;
- execute only small supported subsets behind explicit options;
- fall back to the current IR interpreter for unsupported or verifier-failing
  cases.

Phase 09.03 adds the initial dense-bytecode module under
`crates/php_vm/src/bytecode/`. It lowers a tiny safe subset from rich IR:
`nop`, `load_const`, `move`, `load_local`, `store_local`, `binary add`,
`binary concat`, `echo`, jump terminators, and simple non-reference returns.
The verifier checks register, local, constant, jump target, block terminator,
span side-table, cache-slot, operand-shape, and source-map consistency. The
current VM execution path is unchanged by default.

Phase 09.04 adds explicit execution-format control:
`php-vm run --exec-format=ir|auto|bytecode`. The default is `ir`. Strict
`bytecode` mode lowers and verifies the whole unit, executes only the tiny
Phase 09.03 subset, and returns an unsupported status for unsupported
instruction families. `auto` mode attempts the same lowering and verifier path
but falls back to the rich-IR interpreter when dense bytecode is unsupported.
The `bytecode-exec-smoke` gate compares strict bytecode against IR for
supported fixtures and verifies `auto` fallback plus strict unsupported status.

Phase 09.05 starts coverage expansion without adding semantic shortcuts. Dense
bytecode now lowers and executes scalar binary operations, unary operations, and
comparisons by delegating to the existing rich-IR VM semantic helpers. It also
lowers simple direct positional user-function calls and delegates execution to
the existing VM function-call target path. The A/B smoke covers scalar
expression, comparison, and simple direct-call fixtures in strict bytecode mode.
Builtin dispatch, named/unpacked/by-reference call metadata, property access,
array dimensions, and foreach remain on the unsupported/fallback path until
their instruction-family fixtures and counters land.

Phase 09.06 adds the first default-off dense-bytecode superinstruction pass.
`php-vm run --superinstructions=off|on` controls selection; the default is
`off`. The selector fuses adjacent `load_const` plus `echo`, `load_local` plus
`echo`, and `binary_concat` plus `echo` pairs while leaving block and source-map
instruction indexes stable. The dense executor skips the placeholder `nop`
emitted after a fused instruction, records candidates, emitted counts, executed
counts by kind, and deopt/fallback counters, and preserves the same helpers used
by the unfused path. `just superinstruction-smoke` compares bytecode off/on
behavior and is included in `verify-performance`.

Phase 09.07 hardens the existing request-local frame pool. Frames now carry an
explicit reuse-eligibility flag, so all normal `pop_recycle()` exits obey the
activation decision made at call entry. Simple user-function activations can
reuse completed frames and their register/local-file allocations. Conservative
fresh-frame fallback is used for generators, generator/fiber continuations,
by-reference params or returns, closure captures, class/method contexts, shared
top-level locals, try/finally bodies, and object-allocation bodies that may hold
destructor-sensitive values. Counters now include the prompt-facing
`frames_allocated`, `frames_reused`, `register_files_allocated`,
`register_files_reused`, and `frame_reuse_blocked_by_reason` fields while
keeping the legacy `frame_allocations` and `frame_reuses` keys.

### Tier 1: Quickened Interpreter, Inline Caches, And Superinstructions

Tier 1 specializes observed hot sites while keeping generic interpreter fallback
available. It includes:

- per-site quickening states and dequickening thresholds;
- monomorphic and capped-polymorphic inline caches for calls, builtins,
  properties, arrays, includes, and autoload where proven safe;
- superinstruction selection for measured adjacent bytecode patterns;
- counters for candidates, emitted specializations, hits, misses, guard
  failures, deopts, and fallback reasons.

Phase 09.09 implements the first guarded function/builtin-call slice:
monomorphic function-call ICs now guard call shape and builtin metadata, and the
only builtin fast stubs are exact `strlen`, `count`, `is_int`, `is_string`, and
`is_array` cases that fall back to the existing registry path on mismatch.

Phase 09.11 implements the safe initial packed-array slice: runtime-owned
packed metadata, guarded interpreter packed fetch/append/by-value foreach paths,
stable fallback counters for bounds, layout, COW, and reference cases, and
focused fixtures. Dense-bytecode array execution, by-reference foreach, mutation
during foreach, numeric-string key coercion shortcuts, and reductions remain
generic until separately proven.

Tier 1 must be disable-able by flags such as quickening off, inline caches off,
bytecode execution off, superinstructions off, and tiering off.

### Tier 2a: Optional Baseline Native Tier Research

Tier 2a is research only until a dedicated prompt implements a mock or
no-exec prototype and records a recommendation. It should compare:

- copy-and-patch or stencil baseline JIT;
- partial-evaluation-derived baseline JIT;
- Cranelift-only tiering;
- interpreter plus quickening only.

No executable baseline-native tier may become default without a documented
executable-memory policy, W^X/mprotect policy, ABI hash, helper registry,
side-exit records, live-state maps, and complete reference/COW/foreach,
try/finally, generator, and fiber representation.

### Tier 2b: Selective Cranelift For Proven Hot Regions

Tier 2b uses the existing Cranelift work only after lower tiers provide stable
metadata:

- packed-array kind stability;
- IC stability score;
- low guard-failure score;
- loop/function hotness;
- no reference, COW, mutation, or destructor ambiguity;
- compile budget and process-local cache controls.

It remains feature-gated and runtime-off by default.

## Acceptance Policy

Every acceleration change must satisfy these rules:

- No wall-clock-only correctness decisions.
- Output, stderr, exit status, runtime diagnostics, exception classes, warnings,
  notices, side effects, and source spans where relevant must match the
  baseline.
- Every optimized path has an off switch.
- Every optimized path has a generic fallback or an explicit strict-mode
  unsupported error.
- Every optimized path exposes counters for attempts, successes, hits, misses,
  fallbacks, guard failures, deopts, and skip reasons as applicable.
- Every optimized path has A/B fixtures or differential tests comparing the
  baseline and enabled mode.
- Speed claims are local and advisory unless backed by a stable benchmark
  report with methodology, warmups, repetitions, environment capture, and
  correctness parity.
- Production profile measurements must use the explicit release/profiling Cargo
  profiles and write reports under `target/performance/release/`; PGO and BOLT
  remain optional host/tool-dependent flows with explicit skip reports.
- Generated reports under `target/` are evidence, not committed artifacts.
- PHPT/reference-sensitive changes must run the source-integrity gate and strict
  reference behavior when `REFERENCE_PHP` is explicitly set.

## Phase Order

The prompt-pack order is:

1. Phase 09.00: this plan and acceleration gap catalog.
2. Phase 09.01: representative performance corpus and richer counters. The
   framework-like corpus is now wired into `verify-performance`.
3. Phase 09.02: release, PGO, and optional BOLT measurement infrastructure. The
   release smoke is now part of `verify-performance`; PGO and BOLT are
   skip-safe optional recipes.
4. Phase 09.03 through 09.06: dense bytecode, optional execution, coverage, and
   superinstructions. Phase 09.03 has landed the dense representation and
   verifier skeleton. Phase 09.04 has landed default-off execution for a tiny
   verified subset plus A/B and fallback smoke coverage. Phase 09.05 has
   expanded scalar/direct-call bytecode coverage, and Phase 09.06 has landed the
   default-off superinstruction selector and smoke gate.
5. Phase 09.07 through 09.12: frame/register reuse, quickening sites,
   call/builtin ICs, property ICs, packed-array fast paths, and output/string
   fast paths. Phase 09.07 has landed explicit frame/register reuse
   eligibility, blocked-reason counters, and focused safety fixtures. Phase
   09.08 has landed unified IR/dense quickening site keys, dense int arithmetic,
   dense string concat, and dense bool-branch specialization with shared guard
   fallback counters and dense A/B smoke coverage. Phase 09.09 has landed
   guarded function/builtin call ICs and exact builtin stubs for the initial
   safe subset. Phase 09.10 has landed explicit interpreter property-fetch
   layout metadata, shape guard fixtures, and fallback reason counters; property
   assignment ICs remain future work.
6. Phase 09.13: optimizer pass expansion. The performance pipeline now reports
   safe constant folding, literal-pool compaction, block-local register copy
   propagation, peepholes, and branch simplification with attempted/applied/
   skipped counters plus verifier-backed rollback.
7. Phase 09.14: Cranelift policy hardening and packed-loop expansion only. The
   Cranelift tier remains feature-gated and runtime-off, with packed foreach
   integer reductions as the only keep-and-expand win.
8. Phase 09.15: baseline native tier research/prototype. The
   `docs/research/baseline-native-tier.md` decision keeps baseline native work
   research-only until executable-memory, ABI, deopt, live-state, references,
   COW, foreach, exception, generator, and fiber state are owned by the VM.
9. Phase 09.16: end-to-end acceleration matrix. The `acceleration-matrix`
   recipe writes local JSON/Markdown under `target/performance/acceleration/`
   and feeds the committed summary in `docs/performance-acceleration-results.md`.
10. Phase 09.17: runtime, stdlib, performance, and PHPT compatibility sweep.

Only after Phase 09.01 lands should independent lanes split, and shared edits to
`crates/php_vm/src/vm.rs`, `crates/php_vm/src/counters.rs`, `justfile`, and
`scripts/performance/` must be coordinated manually.

## Acceleration Gap Catalog

The acceleration-specific gaps are tracked in
`docs/performance-acceleration-known-gaps.md`:

- `ACCEL-GAP-REAL-WORKLOAD-CORPUS`
- `ACCEL-GAP-DENSE-BYTECODE`
- `ACCEL-GAP-SUPERINSTRUCTIONS`
- `ACCEL-GAP-FRAME-REUSE`
- `ACCEL-GAP-CALL-BUILTIN-ICS`
- `ACCEL-GAP-PROPERTY-SHAPE-ICS`
- `ACCEL-GAP-PACKED-ARRAY-FASTPATHS`
- `ACCEL-GAP-OUTPUT-BUFFER-FASTPATH`
- `ACCEL-GAP-RELEASE-PGO-BOLT`
- `ACCEL-GAP-BASELINE-JIT-RESEARCH`

Closing any gap requires direct evidence from the owning phase.

## Validation For This Phase

Phase 09.00 is documentation-only. The baseline discovery command for this
phase is:

```bash
nix develop -c just help
```

Result: passed on 2026-06-27 in the `performance` branch. The command exposed
the current canonical validation and performance gate surface. No Rust code,
scripts, fixtures, PHPT manifests, or reference files were changed by this
phase, so clippy, runtime gates, performance gates, and PHPT source-integrity
checks are reserved for later code-changing phases.
