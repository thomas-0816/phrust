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
`frame_allocations`/`frame_reuses` and prompt-facing `frames_allocated`,
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
deferred until each shape has differential fixtures. Cranelift remains
default-off and may only consume this proven metadata through guarded helper
paths; it is not the owner of packed-array semantics.

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
score. Tier 0 is the baseline interpreter, Tier 1 is the quickened interpreter,
and Tier 2 is the default-off experimental JIT when the `jit-cranelift` feature
and `--jit=on` are both enabled. `--tiering=off` disables adaptive quickening
observations and JIT attempts for the request. `--tiering-stats-json <path>`
writes stats outside PHP stdout.

### Experimental JIT

JIT work is default-off and feature-gated. The initial scope is a tiny safe
subset such as pure integer leaf functions. The interpreter remains the source
of truth, JIT eligibility must reject unsupported code, and fallback must be
available for every failure mode. See
`docs/adr/0076-cranelift-jit-experiment.md` and
`docs/performance-jit-experiment.md` for the Performance decision, scope, ABI boundary,
guard/deopt policy, code-cache lifecycle, platform limits, and abort criteria.

## Correctness Contract

- `--opt-level=0` is the baseline when optimization flags exist.
- `--quickening=off`, `--inline-caches=off`, `--bytecode-cache=off`,
  `--exec-format=ir`, `--superinstructions=off`, and `--jit=off` are required
  once the respective layers exist.
- `--tiering=off` must keep adaptive quickening and JIT tier decisions inactive.
- Optimized and baseline runs must match output, stderr, exit status,
  diagnostics, exception classes, warning text where modeled, and
  timing-independent side effects.
- Guard failure falls back to the generic path.
- Cache miss, cache corruption, stale fingerprint, unsupported IR, and
  unsupported JIT platform must degrade to safe baseline behavior.
- Any known deviation is documented in `docs/performance-known-gaps.md`.

## Roadmap

The current staged acceleration program is coordinated by
`docs/performance-acceleration-plan.md` and its gap catalog
`docs/performance-acceleration-known-gaps.md`.

The next current-state fastest-engine work is coordinated by
`docs/performance-fastest-engine-plan.md` and
`docs/performance-fastest-engine-known-gaps.md`. Those docs start from the
already-landed acceleration surface, keep `--exec-format=ir` as the correctness
baseline, and require correctness-first evidence before any new speed claim.

- `07.00`: preflight and initial known-gap catalog.
- `07.01`: scope ADR and performance principles.
- `07.02` to `07.03`: Nix/tooling and Performance justfile gates.
- `07.04` to `07.09`: metrics crate, benchmark corpus, runner, counters,
  baseline/compare tooling, and hot-path inventory.
- `07.10`: IR/bytecode verifier hardening.
- `07.11` to `07.16`: bytecode-cache design, crate, fingerprinting,
  roundtrip, CLI integration, and lifecycle documentation.
- `07.17` to `07.22`: optimizer framework, differential harness, constant
  folding, peepholes, CFG cleanup, and literal pool/string interning.
- `07.23` to `07.27`: quickening model, framework, and selected
  specializations for integer add, string concat, and packed array dim fetch.
- `07.28` to `07.35`: inline-cache design, slots, stats, and caches for
  functions, methods, properties, class constants, static properties, includes,
  and Composer/autoload lookup.
- `07.36` to `07.43`: runtime fast paths and a unified deopt/fallback protocol.
- `07.44` to `07.47`: stress regressions, optional callgrind smoke, Criterion
  hot-path benchmarks, and performance report generation.
- `07.48` to `07.55`: experimental Cranelift JIT ADR, crate, eligibility, ABI,
  lowering, execution smoke, tiering, and safety audit.
- `07.56` to `07.60`: A/B flag matrix, performance result consolidation,
  developer docs, CI/Nix hardening, final audit, and future runtime handoff.
- `07.A` to `07.F`: optional profiling workflow, optional LTO/PGO plan,
  shared-cache research, polymorphic inline caches, framework-like smokes, and
  optional W^X/mprotect JIT memory prototype.

## Reference Links

- `php-src/php-8.5.7/Zend/zend_vm_def.h`
- `php-src/php-8.5.7/ext/opcache/`
- `php-src/php-8.5.7/ext/opcache/jit/README.md`
- PEP 659: https://peps.python.org/pep-0659/
- Cranelift: https://cranelift.dev/
- Criterion.rs: https://bheisler.github.io/criterion.rs/book/
- iai-callgrind: https://docs.rs/iai-callgrind

## Validation Policy

Documentation-only work items use the strongest available docs or smoke gate. If no
dedicated docs gate exists, the current fallback is:

```bash
nix develop -c just verify-stdlib
```

Code-changing Performance work items must add or update a narrower Performance gate and
then include that gate in `verify-performance` once the gate exists.

## Command Surface

Work item introduced the Performance recipes below. Later work items replaced the
initial scaffolding with concrete cache, optimizer, quickening, inline-cache,
JIT, safety, matrix, and reporting gates.

| Command | Current behavior |
| --- | --- |
| `verify-performance` | Runs `performance-tests`, `performance-regression`, cache/optimizer/superinstruction/quickening/inline-cache/JIT/safety gates, benchmark smoke, framework smoke, release benchmark smoke, acceleration matrix, fast-preset smoke, hot-path inventory, and `perf-report`. |
| `performance-tests` | Runs `cargo test --workspace` with deterministic `RUST_MIN_STACK` defaulting to `8388608`, plus Performance script self-tests. |
| `performance-regression` | Runs `scripts/performance_regression_smoke.sh`, then `scripts/performance/regression_smoke.sh` across opt levels 0/1/2, quickening off/on, and inline caches off/on for the Work item stress fixtures, followed by `perf-flag-matrix`. |
| `perf-flag-matrix` | Compares baseline output/exit/stderr against opt 1, opt 2, superinstructions-on-with-IR, quickening, quickening with `--exec-format=auto`, inline caches, bytecode-cache read/write, and all-non-JIT-on combinations across Performance regressions and selected Runtime semantics fixtures. JIT is opt-in with `PHRUST_PERF_MATRIX_JIT=1` when feature/platform support is available. |
| `benchmark-smoke` | Builds the VM, runs deterministic Performance smoke fixtures, checks expected output, and writes `target/performance/benchmark-smoke.json`. |
| `framework-smoke` | Builds the VM, compares opt-off and opt-on runs over deterministic framework-like fixtures, checks output parity, writes `target/performance/framework-smoke/summary.json`, and regenerates `docs/performance-framework-corpus.md`. |
| `acceleration-matrix` | Builds the VM, compares `baseline-ir` against dense-bytecode auto/strict subset, superinstructions, optimizer levels 1/2, quickening, inline caches, all-non-JIT, release, and optional Cranelift rows. It checks stdout, stderr/runtime diagnostics, exit status, and counter sanity before writing local JSON/Markdown under `target/performance/acceleration/`. |
| `fast-preset-smoke` | Builds the VM, compares `--engine-preset=baseline` against `--engine-preset=fast` across selected runtime, stdlib, performance, framework, and local PHPT smoke cases, records fallback/deopt counters, writes local JSON/Markdown under `target/performance/fast-preset/`, and keeps default-on promotion deferred until broader evidence approves it. |
| `bytecode-exec-smoke` | Builds the VM, compares `--exec-format=ir` and strict `--exec-format=bytecode` for the supported dense-bytecode subset including scalar expressions, comparisons, direct user/builtin calls, packed-array dim/append loops, and framework-like mixed-array foreach traversal, verifies `--exec-format=auto` fallback on an unsupported fixture, and writes `target/performance/bytecode-exec-smoke/summary.json`. |
| `superinstruction-smoke` | Builds the VM, compares strict dense bytecode with `--superinstructions=off` and `--superinstructions=on` across supported fixtures, asserts fused opcode/candidate/skip counters, refreshes `docs/performance-superinstructions.md`, and writes `target/performance/superinstruction-smoke/summary.json`. |
| `superinstruction-patterns` | Mines adjacent opcode pairs/triples from strict dense-bytecode lowering, writes local reports under `target/performance/superinstructions/`, and refreshes the concise committed summary in `docs/performance-superinstructions.md`. |
| `release-benchmark-smoke` | Builds `php-vm` with Cargo `profile.release`, runs the deterministic performance and framework corpora against the release binary, and writes `target/performance/release/release-summary.{json,md}` plus corpus reports. Timings are advisory. |
| `pgo-benchmark-smoke` | Optional PGO flow. Without `PHRUST_RUN_PGO=1` or `llvm-profdata`, writes a skip report under `target/performance/release/`; when enabled, builds profile-generate/profile-use release binaries and reruns the corpora. |
| `bolt-benchmark-smoke` | Optional Linux-only BOLT flow. It writes a skip report outside Linux or without `PHRUST_RUN_BOLT=1`, `PHRUST_BOLT_PERF_DATA`, `perf2bolt`, and `llvm-bolt`; enabled runs consume perf data and benchmark the optimized binary. |
| `callgrind-smoke` | Optional Callgrind smoke; skips cleanly outside Linux or without `valgrind`, otherwise writes `target/performance/callgrind/summary.json`. |
| `fastest-hotpath-report` | Builds or reuses the VM, consumes benchmark smoke, framework smoke, acceleration matrix, standalone counter JSON, and optional profiler artifacts, writes `target/performance/fastest/hotpath-report.{json,md}`, and refreshes `docs/performance-fastest-hotpaths.md`. |
| `rust-hotpath-bench` | Runs Criterion benchmarks from the benchmark-only, workspace-excluded `php_bench` package for Rust hot paths. |
| `benchmark-suite` | Runs the deterministic CLI benchmark matrix and then `rust-hotpath-bench`. |
| `perf-baseline` | Builds the VM and writes a local host-specific baseline to `target/performance/baseline.json`. |
| `perf-compare` | Compares `target/performance/baseline.json` with a fresh benchmark smoke and writes `target/performance/perf-compare.md` plus JSON. |
| `cache-roundtrip` | Runs fingerprint smoke coverage, bytecode-cache roundtrip/verifier/corrupt fallback tests, and CLI cache hit/miss/path-component tests. |
| `optimizer-diff` | Verifies IR invariants, compares opt levels 0, 1, and 2 across optimizer fixtures with output, exit, and diagnostic diffs, and archives per-fixture optimizer pass reports under `target/performance/optimizer-diff/optimizer-reports/`. |
| `quickening-smoke` | Builds the VM, compares `--quickening=off` and `--quickening=on` across Performance smoke fixtures plus generated strict dense-bytecode fixtures for int arithmetic, string concat, and bool branches, and asserts quickening and bytecode counters. |
| `inline-cache-smoke` | Builds the VM, compares `--inline-caches=off` and `--inline-caches=on` across Performance smoke fixtures, and asserts IC slots, guarded function-call and builtin-call hits/misses, capped polymorphic dynamic function-call entries with megamorphic fallback, builtin fast-stub hit/miss/fallback-reason attribution, method-call hits/misses/polymorphic hits/guard failures/direct dispatch/tiny-inline metadata, property-fetch hits/misses and shape guard failures, property-assignment hits/misses plus visibility/type/readonly/hook-magic/dynamic fallback reasons, class/static hits/misses, and include/eval/autoload epoch invalidation counters. |
| `jit-smoke` | Runs default-off `php_jit` API, eligibility, ABI, optional Cranelift lowering tests, feature-on VM JIT tests, and a CLI A/B smoke comparing `--jit=off` and `--jit=on`; asserts compile/execution/fallback counters while keeping native machine-code execution disabled. |
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
