# Performance Performance Principles

Performance adds a correctness-preserving performance layer to the PHP 8.5.7 Rust
engine. It does not redefine PHP semantics, replace the existing frontend or VM,
or treat benchmark wins as a substitute for Foundation through Standard library regression
proof.

## Optimization Layers

### Measurement And Benchmark Infrastructure

The first layer establishes deterministic fixtures, machine-readable metrics,
environment capture, baseline generation, comparison tooling, counters, and
reports. Wall-clock data is advisory unless paired with stable methodology and
clear uncertainty.

### Bytecode And IR Cache

The cache stores verified compiled artifacts with fingerprints covering source,
engine version, target PHP version, format versions, options, and relevant
configuration. Corrupt or stale artifacts must fall back to compile-from-source.

### Optimizer Pass Framework

Optimizer passes run behind explicit opt levels. `--opt-level=0` is the semantic
baseline. Higher levels may add safe constant folding, peepholes, and CFG
simplification only when verifier and A/B tests prove no visible behavior
changes.

The current Performance pipeline runs verifier-bracketed passes for safe
constant folding, literal-pool compaction, block-local register copy
propagation, NOP/self-move peepholes, and conservative CFG simplification.
Pass reports are machine-readable and include stable attempted/applied/skipped
style counters plus skip-reason counters for non-literal and unsafe fold
candidates. Constant folding includes integer arithmetic, exact string concat,
boolean not, and a strict literal-compare subset: integer relational and
spaceship comparisons, bool/int/null same-type equality, and scalar strict
identity. It intentionally skips loose string/numeric-string, float, array,
object, local/reference, and call-derived cases. Literal compaction
deduplicates equal constants and remaps all constant IDs in instructions,
terminators, attributes, class metadata, and global constants. Copy propagation
is intentionally register-only and block-local; it does not cross
local/reference state or basic-block boundaries.

### Quickening

Quickening may rewrite or side-table op behavior after hotness evidence, but
specialized paths must preserve fallback to the generic interpreter path. Guard
misses, overflow, type changes, by-reference behavior, exceptions, and other PHP
edge cases must deopt or remain unspecialized.

### Inline Caches

Inline caches may cache resolution results for functions, methods, properties,
class constants, static properties, include paths, autoload lookups, and internal
dispatch only when guarded by class, function, method-table, property-shape,
autoload, include-path, and configuration epochs as appropriate.

### Runtime Fast Paths

Runtime fast paths may optimize local slots, frame reuse, arrays, numeric-string
classification, parameter prologues, internal functions, and output buffering.
They must not bypass references, COW, destructors, generators, fibers,
exceptions, visibility, magic methods, or diagnostics.

Frame/register reuse is request-local and conservative. The VM reuses completed
plain user-function frames and their register/local-file allocations, but pushes
fresh non-pooled frames for closure captures, by-reference params or returns,
generator/fiber continuations, class contexts, shared top-level locals,
try/finally bodies, and object-allocation bodies that may retain
destructor-sensitive values. Raw counters expose both legacy
`frame_allocations`/`frame_reuses` and compatibility `frames_allocated`,
`frames_reused`, `register_files_allocated`, `register_files_reused`, and
`frame_reuse_blocked_by_reason`.

Runtime layout compactness work is measured before broad representation
changes. The VM now collects request-local layout counters for `value_clones`,
`string_allocations`, `array_handle_clones`, `cow_separations`,
`reference_cell_creations`, and `object_allocations`; reports surface those
alongside frame/register reuse and literal-pool intern counters. The first local
win builds known packed arrays with exact storage in `PhpArray::from_packed`
instead of routing through repeated generic appends. Public `Value`, string,
array, object, and reference APIs remain unchanged, and future compactness work
must preserve COW, references, destructor order, and diagnostic order.

Packed-array fast paths consume `PhpArray::packed_metadata()` rather than
duplicating layout knowledge in the VM. The metadata records packed-vs-mixed
kind, direct element summary, key-kind summary, numeric-string key ambiguity,
direct reference elements, COW sharing, mutation epoch, and packed length.
Internally `PhpArray` now separates packed and mixed storage variants behind
that facade; the VM/JIT boundary is unchanged and must not inspect the private
storage enum. Packed storage still keeps key/value entries to preserve borrowed
key iteration, so a values-only packed buffer and additional mixed hash indexes
are deferred implementation details rather than public contracts.
Read-only packed int fetches may run on shared handles, because operand reads
clone the array handle, but mutation-sensitive append/assign paths and
reference-bearing arrays record `cow_or_reference_fallbacks` and use generic
semantics. The VM also exposes `array_fast_path_hits_by_family` and
`array_fast_path_fallback_by_reason` so smoke and matrix gates can distinguish
packed fetch, append, foreach, reduction, COW/reference, bounds, layout, and
numeric-string-key behavior. The current packed-int reduction helper is guarded
for packed all-int, non-shared, non-reference arrays and exact overflow
behavior; broader builtin `array_sum`/`min`/`max` specialization remains
deferred until each shape has differential fixtures. The guarded native tier may
consume this proven metadata through VM-owned helper paths; it is not the owner
of packed-array semantics.

Output and string fast paths stay byte-exact. The VM may coalesce adjacent
exact-output IR `Echo` operations and immediate `LoadConst`/`Echo` producer
pairs only when every value is already a direct string, integer, `true`,
`false`, or `null`; generic conversion, diagnostics, calls, branches, output
buffer control, references, objects, arrays, resources, callables, generators,
fibers, and uninitialized values remain on the existing path. Counters expose
`output_fast_appends`, `output_batched_appends`, `output_batch_bytes`,
`output_slow_appends_by_reason`, `concat_prealloc_hits`, and
`concat_fallback_by_reason`. Generic concat reserves exact byte capacity only
after normal conversions succeed, so `__toString`, warnings, and errors keep
their existing order.

### Tiering Policy

Tiering is request-local and advisory. The current policy tracks function entry
count, loop backedge count, inline-cache stability score, and guard-failure
score. Tier 0 is the generic interpreter, Tier 1 is the dense/quickened
interpreter with guarded inline caches, and Tier 2 is the guarded native
hot-region tier when the backend and platform support it. The default managed
runtime requests Tier 2 automatically for eligible regions; unsupported
platforms and rejected shapes record counters and keep running Tier 0/Tier 1
paths. `--tiering=off` is a developer diagnostic switch that disables adaptive
quickening observations and native-tier decisions for the request.
`--tiering-stats-json <path>` writes stats outside PHP stdout.

### Guarded Native Tier

The native tier is part of the managed runtime, not a user-facing tuning mode.
It is limited to narrow regions such as counted integer loops, packed all-int
foreach reductions, exact scalar leaf functions, exact builtin intrinsic
regions, and simple scalar branches. Live-state snapshots are the only
optimized-exit mechanism. Code cache keys include helper ABI/version hashes,
runtime configuration hashes, invalidation epochs, and internal compile-budget
state. Side exits materialize the live snapshot and resume generic execution for
guard failure, overflow, type change, reference/COW, shape or epoch
invalidation, output/diagnostic boundaries, and exception boundaries. See
`docs/adr/0017-cranelift-jit-experiment.md` and
`docs/performance/jit-experiment.md` for the compiler scope, ABI boundary,
guard/deopt policy, code-cache lifecycle, platform limits, and abort criteria.

## Correctness Contract

- `--opt-level=0` is the baseline when optimization flags exist.
- `--quickening=off`, `--inline-caches=off`, `--bytecode-cache=off`,
  `--exec-format=ir`, `--superinstructions=off`, and `--jit=off` remain
  developer diagnostics and baseline rollback controls.
- `--tiering=off` must keep adaptive quickening and native-tier decisions inactive.
- Optimized and baseline runs must match output, stderr, exit status,
  diagnostics, exception classes, warning text where modeled, and
  timing-independent side effects.
- Guard failure falls back to the generic path.
- Cache miss, cache corruption, stale fingerprint, unsupported IR, and
  unsupported native backend/platform must degrade to safe managed interpreter
  behavior.
- Any known deviation is documented in `docs/performance/known-gaps.md`.

## Current Performance Surfaces

The current performance documentation is organized around stable contracts,
current command behavior, generated summaries, and known gaps:

- `docs/performance/methodology.md`: measurement and reporting policy.
- `docs/performance/runtime.md`: runtime and VM optimization contracts.
- `docs/performance/bytecode-cache.md`: CLI bytecode cache behavior.
- `docs/reference/performance-status.md`: committed local benchmark summary.
- `docs/performance/framework-corpus.md`: generated framework-smoke summary.
- `target/performance/fastest/matrix.md`: generated fastest-engine matrix
  summary.
- `docs/performance/known-gaps.md` and
  `docs/known_gaps/performance.jsonl`: human and machine-readable gap catalogs.

Generated raw performance artifacts stay under `target/performance/` and are
not committed. Current `just` recipes are the source of truth for command names
and gate membership.

## Reference Links

- `php-src/php-8.5.7/Zend/zend_vm_def.h`
- `php-src/php-8.5.7/ext/opcache/`
- `php-src/php-8.5.7/ext/opcache/jit/README.md`
- PEP 659: https://peps.python.org/pep-0659/
- Cranelift: https://cranelift.dev/
- Criterion.rs: https://bheisler.github.io/criterion.rs/book/
- iai-callgrind: https://docs.rs/iai-callgrind

## Validation Policy

Documentation-only performance changes use the strongest available docs or
smoke gate. If no dedicated docs gate exists, the current fallback is:

```bash
nix develop -c just verify-stdlib
```

Code-changing performance changes must add or update a narrower Performance
gate and then include that gate in `verify-performance` once the gate exists.

## Command Surface

The performance recipes below are the current command surface. Earlier
scaffolding has been replaced with concrete cache, optimizer, quickening,
inline-cache, JIT, safety, matrix, and reporting gates.

| Command | Current behavior |
| --- | --- |
| `verify-performance` | Runs `performance-tests`, `performance-regression`, cache/optimizer/superinstruction/quickening/inline-cache/native/safety gates, benchmark smoke, framework smoke, release benchmark smoke, acceleration matrix, default-profile smoke, managed-fast coverage, fast-preset smoke, hot-path inventory, and `perf-report`. |
| `performance-tests` | Runs `cargo test --workspace` with deterministic `RUST_MIN_STACK` defaulting to `8388608`, plus Performance script self-tests. |
| `performance-regression` | Runs `scripts/performance_regression_smoke.sh`, then `scripts/performance/regression_smoke.sh` across opt levels 0/1/2, quickening off/on, and inline caches off/on for the performance stress fixtures, followed by `perf-flag-matrix`. |
| `perf-flag-matrix` | Compares baseline output/exit/stderr against opt 1, opt 2, superinstructions-on-with-IR, quickening, quickening with `--exec-format=auto`, inline caches, bytecode-cache read/write, and all-non-JIT-on combinations across Performance regressions and selected Runtime semantics fixtures. Low-level JIT rows are opt-in with `PHRUST_PERF_MATRIX_JIT=1` when feature/platform support is available. |
| `benchmark-smoke` | Builds the VM, runs deterministic Performance smoke fixtures, checks expected output, and writes `target/performance/benchmark-smoke.json`. |
| `framework-smoke` | Builds the VM, compares opt-off and opt-on runs over deterministic framework-like fixtures, checks output parity, writes `target/performance/framework-smoke/summary.json`, and regenerates `docs/performance/framework-corpus.md`. |
| `acceleration-matrix` | Builds the VM, compares `baseline-ir` against dense-bytecode auto/strict subset, superinstructions, optimizer levels 1/2, quickening, inline caches, all-non-JIT, release, and optional Cranelift rows. It checks stdout, stderr/runtime diagnostics, exit status, and counter sanity before writing local JSON/Markdown under `target/performance/acceleration/`. |
| `default-profile-smoke` | Builds the VM, compares `--engine-preset=baseline` against `--engine-preset=default` across selected runtime, stdlib, performance, framework, and local PHPT smoke cases, records fallback/deopt counters, checks managed fast-path and native availability/execution counters, and writes local JSON/Markdown under `target/performance/default-profile/`. |
| `managed-fast-coverage` | Builds the VM, runs curated default-profile fixtures, asserts dense bytecode, superinstructions, quickening, inline caches, array shapes, builtin intrinsics, string/output batching, include/cache reuse, native-tier policy, and bounded fallback counters, and writes local JSON/Markdown under `target/performance/managed-fast/`. |
| `fast-preset-smoke` | Builds the VM, compares `--engine-preset=baseline` against `--engine-preset=fast` across selected runtime, stdlib, performance, framework, and local PHPT smoke cases, records fallback/deopt counters, and writes local JSON/Markdown under `target/performance/fast-preset/` for compatibility-alias coverage. |
| `bytecode-exec-smoke` | Builds the VM, compares `--exec-format=ir` and strict `--exec-format=bytecode` for the supported dense-bytecode subset including scalar expressions, comparisons, direct user/builtin calls, packed-array dim/append loops, and framework-like mixed-array foreach traversal, verifies `--exec-format=auto` fallback on an unsupported fixture, and writes `target/performance/bytecode-exec-smoke/summary.json`. |
| `superinstruction-smoke` | Builds the VM, compares strict dense bytecode with `--superinstructions=off` and `--superinstructions=on` across supported fixtures, asserts fused opcode/candidate/skip counters, refreshes `docs/performance/superinstructions.md`, and writes `target/performance/superinstruction-smoke/summary.json`. |
| `superinstruction-patterns` | Mines adjacent opcode pairs/triples from strict dense-bytecode lowering, writes local reports under `target/performance/superinstructions/`, and refreshes the concise committed summary in `docs/performance/superinstructions.md`. |
| `release-benchmark-smoke` | Builds `php-vm` with Cargo `profile.release`, runs the deterministic performance and framework corpora against the release binary, and writes `target/performance/release/release-summary.{json,md}` plus corpus reports. Timings are advisory. |
| `pgo-benchmark-smoke` | Optional PGO flow. Without `PHRUST_RUN_PGO=1` or `llvm-profdata`, writes a skip report under `target/performance/release/`; when enabled, builds profile-generate/profile-use release binaries and reruns the corpora. |
| `bolt-benchmark-smoke` | Optional Linux-only BOLT flow. It writes a skip report outside Linux or without `PHRUST_RUN_BOLT=1`, `PHRUST_BOLT_PERF_DATA`, `perf2bolt`, and `llvm-bolt`; enabled runs consume perf data and benchmark the optimized binary. |
| `callgrind-smoke` | Optional Callgrind smoke; skips cleanly outside Linux or without `valgrind`, otherwise writes `target/performance/callgrind/summary.json`. |
| `fastest-hotpath-report` | Builds or reuses the VM, consumes benchmark smoke, framework smoke, acceleration matrix, standalone counter JSON, and optional profiler artifacts, writes `target/performance/fastest/hotpath-report.{json,md}`, and refreshes `target/performance/fastest/hotpath-report.md`. |
| `rust-hotpath-bench` | Runs Criterion benchmarks from the benchmark-only, workspace-excluded `php_bench` package for Rust hot paths. |
| `benchmark-suite` | Runs the deterministic CLI benchmark matrix and then `rust-hotpath-bench`. |
| `perf-baseline` | Builds the VM and writes a local host-specific baseline to `target/performance/baseline.json`. |
| `perf-compare` | Compares `target/performance/baseline.json` with a fresh benchmark smoke and writes `target/performance/perf-compare.md` plus JSON. |
| `cache-roundtrip` | Runs fingerprint smoke coverage, bytecode-cache roundtrip/verifier/corrupt fallback tests, and CLI cache hit/miss/path-component tests. |
| `optimizer-diff` | Verifies IR invariants, compares opt levels 0, 1, and 2 across optimizer fixtures with output, exit, and diagnostic diffs, and archives per-fixture optimizer pass reports under `target/performance/optimizer-diff/optimizer-reports/`. |
| `quickening-smoke` | Builds the VM, compares `--quickening=off` and `--quickening=on` across Performance smoke fixtures plus generated strict dense-bytecode fixtures for int arithmetic, string concat, and bool branches, and asserts quickening and bytecode counters. |
| `inline-cache-smoke` | Builds the VM, compares `--inline-caches=off` and `--inline-caches=on` across Performance smoke fixtures, and asserts IC slots, guarded function-call and builtin-call hits/misses, capped polymorphic dynamic function-call entries with megamorphic fallback, builtin fast-stub hit/miss/fallback-reason attribution, method-call hits/misses/polymorphic hits/guard failures/direct dispatch/tiny-inline metadata, property-fetch hits/misses and shape guard failures, property-assignment hits/misses plus visibility/type/readonly/hook-magic/dynamic fallback reasons, class/static hits/misses, and include/eval/autoload epoch invalidation counters. |
| `jit-smoke` | Runs low-level `php_jit` API, eligibility, ABI, optional Cranelift lowering tests, feature-on VM native-tier tests, and CLI A/B diagnostics; asserts compile/execution/fallback counters and explicit platform skips. |
| `safety-audit-smoke` | Scans the Performance cache/JIT/adaptive runtime surface for Rust `unsafe`, runs bytecode-cache negative tests, and runs a small Miri cache test when the active toolchain supports it. |
| `perf-report` | Renders `target/performance/perf-report.md` and JSON from benchmark measurements, VM counters, comparison artifact presence, and known gaps. |

Tiering flags available to `php-vm run`:

```bash
--tiering off|on
--tiering-function-threshold N
--tiering-loop-threshold N
--tiering-ic-stability-threshold N
--tiering-guard-failure-threshold N
--tiering-stats-json target/performance/tiering.json
```

## Contributor Fast-Path Rules

New fast paths must reuse shared semantic helpers instead of duplicating PHP
behavior, keep local fallback to the generic interpreter, expose specific
fallback counters, and add focused fast-hit assertions before joining
`managed-fast-coverage` or the default profile.
