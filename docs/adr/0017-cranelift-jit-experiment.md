# ADR 0017: Cranelift JIT Experiment

## Status

Accepted as an experimental, default-off Performance path.

## Context

Performance is a correctness-preserving optimization layer. The interpreter, IR
verifier, optimizer, quickening layer, inline caches, and runtime fast paths
already provide the semantic baseline for PHP 8.5.7 behavior. A JIT experiment
can only be useful if it consumes the existing frontend-to-runtime pipeline and
never becomes a second frontend, a second semantic model, or the only execution
strategy.

Cranelift is the candidate backend because it is Rust-native, embeddable, has a
stable IR API surface compared with hand-written machine code, and supports
multiple host architectures through one backend. It also lets this repository
test a real native-code path without committing to Zend Opcache JIT internals or
platform-specific assembly in Performance.

## Decision

Performance may add a `jit-cranelift` experiment, default off. The experiment is
allowed to add JIT API scaffolding, eligibility analysis, ABI types, smoke
gates, optional Cranelift-backed IR lowering for a tiny subset, guarded native
entries, helper-call boundaries, and interpreter-fallback-first execution. The
default build must not require Cranelift, executable memory, or platform JIT
support.

The interpreter remains the source of truth. All JIT-eligible code must have an
available interpreter fallback. Any unsupported feature, guard failure, ABI
error, platform limitation, code-cache failure, or safety-audit failure must
reject JIT execution and continue through the interpreter.

## Initial IR Subset

The first eligible subset is intentionally small:

- pure leaf functions or single hot-loop regions;
- primitive integer and boolean operations;
- local-slot reads and writes that do not alias through references;
- simple constants and simple returns;
- no arrays, objects, resources, strings requiring PHP conversion, references,
  copy-on-write mutation, destructors, includes, eval, autoload, generators,
  fibers, exceptions, `try`, `catch`, or `finally`;
- no userland calls and no internal calls except explicitly modeled intrinsics.

Everything outside that subset is rejected with a stable reason. Rejecting too
much is correct; accepting unsupported PHP behavior is not.

## ABI Boundary

The JIT ABI is a narrow boundary between VM-owned state and native code:

- VM context is passed as an opaque handle, not a borrowed Rust reference that
  can escape native code.
- Frame/register access is represented through explicit view types owned by the
  JIT boundary layer.
- Values crossing the boundary use a documented representation that cannot
  bypass reference, COW, GC, visibility, or destructor invariants.
- Native code returns a structured result: normal return, bailout/deopt,
  runtime-callout request, or exception propagation marker.
- Runtime callouts are explicit and must re-enter the interpreter/runtime through
  safe wrappers.

No raw internal Rust reference may be stored by JIT code or survive past the
call boundary. If `unsafe` becomes necessary, it must be isolated in a small
module with invariants documented in `docs/performance/safety-audit.md`.

## Guards and Deoptimization

The first JIT tier uses conservative guards:

- eligibility guards are checked before compile;
- runtime guards are checked before entering native code and at callout points;
- guard failure returns a bailout result and resumes the interpreter at a
  well-defined IR location;
- megamorphic or repeatedly failing regions are disabled for the request or code
  cache epoch;
- guard and bailout counters are emitted so `jit-smoke` can prove fallback is
  exercised without depending on wall-clock timing.

Performance does not require speculative object, array, method, property, reference,
or exception deoptimization.

## Code Cache Lifecycle

The code cache is request-local until a later ADR proves a shared lifecycle. A
cache entry is keyed by IR identity, compiler options, target triple, feature
flags, and invalidation epochs that affect eligibility. Entries are dropped on
source/IR mismatch, unsupported platform, feature disable, failed verification,
or guard instability.

Persistent OPcache-style native code sharing, preloading, process-wide eviction,
and FPM/SAPI worker lifecycle are outside Performance.

## Safety Model

Default builds and tests run with `jit-cranelift` disabled and must not allocate
or execute writable/executable memory. Feature-enabled builds may compile JIT
infrastructure and may execute guarded native entries only when runtime native
execution is explicitly allowed, the backend verifies the lowered region, the
handle carries the expected ABI hash, and the target platform satisfies the
documented executable-memory policy.

The safety model requires:

- no executable-memory path without a local owner type and audit note;
- no native code execution from unverified or stale IR;
- no fallback path that skips PHP diagnostics, destructors, references, COW,
  exceptions, or observable output;
- no hard performance gate based only on wall-clock timings;
- no feature-on behavior that changes feature-off interpreter output.

## Platform Boundaries

The required Performance behavior is portable skip or fallback. A host without
Cranelift support, executable-memory support, or a verified W^X implementation
must compile feature-off and pass `jit-smoke` as skipped/default-off. Feature-on
native execution may be limited to explicitly documented targets after tests
exist for those targets.

## Feature Flag

The experiment uses a default-off feature named `jit-cranelift`. CLI behavior
defaults to managed/interpreter execution. Enabling the feature is not the same
as enabling execution; runtime flags, platform checks, eligibility, helper
availability, ABI hashes, and safety gates must still permit or reject each
region.

The implemented experiment keeps optional dependencies on `cranelift-codegen`
and `cranelift-frontend` behind this feature. It covers backend selection,
eligibility analysis, CLIF verification, helper-backed native entries,
side-exit reporting, blacklist/tiering counters, and guarded native execution
for narrow scalar, array, string, and property shapes. Unsupported runtime
values, guard failures, ABI mismatches, platform limitations, and compile
rejections fall back to interpreter or VM helper paths.

## Abort Criteria

The JIT experiment must stay disabled, be reverted, or be handed off to a later
layer if any of these happen:

- JIT output, stderr, exit code, diagnostics, exception class, or
  timing-independent side effects diverge from interpreter output.
- A bailout cannot resume the interpreter at a proven-safe location.
- ABI values can violate reference, COW, GC, destructor, or visibility
  invariants.
- Executable-memory handling cannot satisfy W^X or platform security rules.
- Feature-off builds pull in Cranelift or require executable-memory support.
- The implementation requires broad frontend, IR, VM, or runtime rewrites.
- The only evidence of benefit is noisy wall-clock data without correctness
  proof.

## Consequences

This decision allows Performance work items to maintain `php_jit`, eligibility,
ABI types, smoke gates, optional Cranelift compilation, and narrow guarded
native execution. It does not authorize a production JIT, default-on native
execution, shared native-code cache, Zend JIT compatibility, or any semantic
shortcut for speed.
