# ADR 0017: Cranelift-Only Production Execution

## Status

Accepted. Migration is governed by the staged cutover gate described below.

## Context

Phrust historically accumulated several execution engines: IR and bytecode
interpreters, dense and rich dispatch, a stencil native tier, and an optional
Cranelift tier. Keeping those engines behaviorally aligned multiplies every
runtime change and leaves production behavior dependent on fallback paths.

The Cranelift path now has process-owned code generations, executable
multi-block Region IR, direct compiled calls, precise native state, native OSR,
and worker prewarming. These prerequisites make it possible to converge on one
execution contract instead of extending the experimental split.

## Decision

The sole production executor will be:

`php_ir -> mandatory executable Region IR -> Cranelift native code`.

Cranelift is mandatory in production builds. Runtime backend selectors,
native-off modes, opcode fallback, interpreter resumption, and alternate native
emitters are removed by the cutover. Unsupported source must fail with a stable
compile-time diagnostic; it must never produce plausible output through a
hidden alternate executor.

The migration is sequential and ratcheted. The pre-cutover revision is pinned
as an external executable oracle. Each cutover prompt must reduce an explicit
temporary source allowlist. The final ratchet permits no legacy executor or
alternate-backend references in production source.

## ABI Boundary

The native ABI is the versioned boundary between runtime-owned state and
generated code:

- VM context is passed as an opaque handle, not a borrowed Rust reference that
  can escape native code.
- Frame/register access is represented through explicit view types owned by the
  JIT boundary layer.
- Values crossing the boundary use a documented representation that cannot
  bypass reference, COW, GC, visibility, or destructor invariants.
- Native code returns a structured result: normal return, native continuation,
  runtime call, deoptimization, or exception propagation marker.
- Runtime calls are explicit typed helpers and return only to native
  continuations.

No raw internal Rust reference may be stored by generated code or survive past
the call boundary. Necessary `unsafe` remains isolated in the audited native
boundary.

## Guards and Deoptimization

Guards and deoptimization preserve precise Region IR state. Deoptimization may
transfer only to another validated native entry or produce a deterministic
diagnostic. It may not transfer into an interpreter or opcode dispatch loop.

## Code Cache Lifecycle

The code manager owns native modules for the process and publishes immutable
handles to workers. Cache identity includes source and Region IR identity,
Region IR schema version, compiler/runtime/helper ABI hashes, Cranelift version,
target triple, CPU feature identity, and invalidation epochs. Warm requests must
reuse validated native code without compiling.

## Safety Model

Production builds allocate and execute code only through Cranelift's audited
memory provider. Region verification, ABI validation, and target validation are
mandatory before publication.

The safety model requires:

- no executable-memory path without a local owner type and audit note;
- no native code execution from unverified or stale IR;
- no alternate executor path that skips PHP diagnostics, destructors,
  references, COW, exceptions, or observable output;
- no hard performance gate based only on wall-clock timings;
- no feature-on behavior that changes feature-off interpreter output.

## Platform Boundary

Supported production targets must have a verified Cranelift ISA and executable
memory policy. An unsupported host is a build or startup error, not permission
to select another engine.

## Consequences

The runtime has one execution semantics and one code generator. The external
pre-cutover oracle remains a differential validation tool only and is never
linked into production. The cutover may temporarily leave explicitly listed
legacy source in place, but no new dependency on that source is allowed.
