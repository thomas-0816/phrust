# Performance JIT Experiment

Performance treats JIT work as an experiment behind a default-off feature. The
decision record is `docs/adr/0017-cranelift-jit-experiment.md`.

## Decision Summary

The project may explore Cranelift in Performance, but only as a conservative
default-off backend for a tiny verified IR subset. The interpreter remains the
source of truth and every JIT path must have an interpreter fallback.
Feature-off builds must not depend on Cranelift or executable memory.

## Why Cranelift

Cranelift is the candidate backend because it is Rust-native, embeddable, and
portable across target architectures without hand-written assembly. It gives the
engine a realistic native-code experiment while preserving the repository's
existing Rust layering and avoiding a dependency on Zend Opcache JIT internals.

## Minimal Scope

Eligible Performance regions are limited to:

- pure leaf functions or single hot-loop regions;
- primitive integer and boolean operations;
- simple local-slot loads and stores;
- constants and simple returns;
- no by-reference parameters, reference cells, COW-sensitive writes, arrays,
  objects, resources, strings requiring PHP conversion, includes, eval,
  autoload, destructors, exceptions, generators, or fibers;
- no calls except explicitly approved intrinsics.

The eligibility rule is reject-first. If a region is not obviously safe, it is
not JIT-eligible.

## JIT ABI

The native boundary is intentionally narrow:

- VM state crosses as opaque handles.
- Frame/register access uses explicit view types.
- Values use a documented boundary representation rather than arbitrary Rust
  references.
- Native code returns one of: normal return, bailout/deopt, runtime callout, or
  exception propagation marker.
- Runtime callouts re-enter the VM through safe wrappers.

No raw VM or runtime reference may escape into native code. Any future `unsafe`
must be isolated and documented in `docs/performance/safety-audit.md`.

## Guards and Fallback

JIT execution is guarded before compile and before native entry. Guard failure,
unsupported IR, stale metadata, unsupported host platform, or code-cache failure
must fall back to interpreter execution. Repeated guard failures disable the
region for the relevant request or cache epoch. Counters must make guard hits,
misses, bailouts, and skips observable.

## Code Cache Lifecycle

The Performance code cache is request-local. Cache keys include IR identity,
compiler options, target triple, feature flags, and invalidation epochs. Shared
native-code caches, OPcache-style preloading, process-wide eviction, and
FPM/SAPI lifecycle are out of scope.

## Safety and Platform Rules

Default builds run with JIT disabled and must not allocate executable memory.
Feature-on builds may compile JIT infrastructure and execute the guarded
the int-leaf prototype through safe VM-owned code after Cranelift IR
verification. Native machine-code execution remains blocked until a W^X or
equivalent executable-memory policy is implemented and audited. Unsupported
platforms must skip or fall back cleanly.

## Feature and CLI Policy

The Cargo feature is `jit-cranelift`, default off. The CLI switch is
`--jit=off|on`, and the runtime default is `--jit=off`. Enabling the feature
only makes the experiment available; eligibility, warmup/hotness, guards, and
runtime flags still decide whether any region can attempt compilation.

The performance layer routes JIT attempts through the request-local tiering policy.
`--tiering=off` prevents JIT compilation attempts even if `--jit=on` is
specified. `--tiering-stats-json <path>` exposes the function-entry,
loop-backedge, IC-stability, guard-failure, and Tier 2 candidate counters used
to explain why a request stayed interpreted or became JIT-eligible.

## Abort Criteria

Stop or hand off the experiment if:

- JIT and interpreter outputs diverge;
- bailout cannot resume at a safe IR location;
- ABI values can violate reference, COW, GC, destructor, or visibility rules;
- executable memory cannot satisfy the safety model;
- feature-off builds pull in JIT dependencies;
- implementation requires broad rewrites outside the JIT layer;
- reported benefit depends only on noisy wall-clock data.

## Validation Surface

The performance layer provides `crates/php_jit` as a default-off API skeleton. The performance layer adds conservative eligibility analysis for a primitive int/bool leaf-function IR
subset with stable rejection and unknown reason codes. The performance layer provides a
safe, handle-based VM/JIT ABI for context/frame views, value boundaries,
bailout/deopt results, runtime callouts, and exception markers. The performance layer adds an optional `jit-cranelift` lowering prototype that converts the tiny
integer subset into verified Cranelift IR text and rejects unsupported IR with
typed errors.

The performance layer provides the first execution integration under both
`--features jit-cranelift` and CLI `--jit=on`. The VM tracks request-local
hotness, attempts compilation only after warmup, calls the Cranelift lowerer as
the compile proof, and then executes only guarded int leaf functions through a
safe integer evaluator. Supported execution covers integer constants, local
loads/stores, moves, add/sub/mul with checked overflow, and integer return.
Calls, arrays, objects, references, typed parameters/returns, generators,
fibers, methods, closures, and non-integer values fall back to the interpreter.
`jit-smoke` now compares `--jit=off` and `--jit=on` output, asserts
`jit_compile_attempts`, `jit_compiled`, `jit_executed`, and `jit_bailouts`, and
keeps `native_machine_code_execution` false.

The performance layer provides the tiering policy and stats surface. The JIT remains
default-off, feature-gated, and limited to eligible hot int-leaf functions; the
tiering layer only controls when the VM may attempt that path.

## Developer Commands

Standard JIT validation:

```bash
nix develop -c just jit-smoke
nix develop -c just safety-audit-smoke
nix develop -c just verify-performance
```

Focused Rust tests:

```bash
nix develop -c cargo test -p php_jit
nix develop -c cargo test -p php_jit --features jit-cranelift
nix develop -c cargo test -p php_vm --features jit-cranelift jit_
```

Manual CLI comparison for the tiny int-leaf fixture:

```bash
nix develop -c cargo build -p php_vm_cli --bin php-vm
nix develop -c target/debug/php-vm run \
  --jit=off \
  tests/fixtures/performance/jit/int-leaf-hot-loop.php
nix develop -c target/debug/php-vm run \
  --jit=on \
  --jit-eager \
  --counters-json target/performance/jit-counters.json \
  tests/fixtures/performance/jit/int-leaf-hot-loop.php
```

The standard CLI build may accept `--jit=on`, but feature-off or unsupported
platform configurations must fall back or skip native execution. Guarded native
entries are experimental, default-off, and not production-ready native JIT.

## Executable-Memory Boundary

Guarded Cranelift native entries allocate executable memory only through
Cranelift's JIT memory provider. The separate repository-owned
`php_jit::code_memory::CodeMemory` abstraction covers emitted machine-code
experiments and is not wired into VM execution.

The follow-up requirement for production/default-on native execution is:

- enforce write-then-execute and never writable+executable mappings where the
  host platform supports that policy;
- add lifecycle, invalidation, and reclamation tests on every supported host
  family;
- keep `--jit=off` as the runtime default and preserve interpreter fallback;
- keep shared or persistent native-code caches blocked until a later ADR owns
  their integrity keys, epochs, and invalidation model.

## Troubleshooting

- Unsupported platform or feature-off build: run `just jit-smoke` and inspect
  the skip/fallback counters. Do not treat feature-off fallback as a failure.
- No compile attempts: check `--jit=on`, tiering thresholds, and
  `--tiering=off`. Tiering disabled means no JIT attempts by design.
- Eligibility rejection: inspect the typed rejection reason from `php_jit`; most
  PHP features intentionally fall back because the accepted subset is tiny.
- Output mismatch: stop expanding the JIT path and compare `--jit=off` versus
  `--jit=on` with the same fixture. Interpreter output is authoritative.
- Native execution claims: keep experimental native-entry support separate from
  production/default-on JIT or OPcache-style claims.
