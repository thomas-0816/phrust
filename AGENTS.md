# Phrust native hot-path replacement contract

This replaces the generic warm runtime. It does not add another
optimization layer around it.

## Generated execution tiers

Every ordinary PHP function has a semantically complete Generic Cranelift
body and may also have an Optimizing Cranelift body. Both tiers use the same
native value representation and generated-function ABI. An optimizing miss or
deoptimization enters Generic Cranelift, never a Rust PHP-function executor.

Rust may remain on cold compile, publication, resolution, extension, FFI,
reflection/debug, and outer-result boundaries. Runtime calls from generated
code must be exact typed leaves; operation-ID helpers are forbidden. Cold Rust
may resolve a userland target but may not execute its PHP body.

The existing warm WordPress contract remains c1 p50 at most 80 ms and p95 at
most 100 ms. The sharper strategic target is at most 2.0x a simultaneously
measured PHP-FPM control; report the absolute contract and ratio separately.

## Non-negotiable architecture

For every operation family in scope, implementation means:

1. direct CLIF;
2. direct access to a stable native data representation;
3. a compiled native call; or
4. one native transition to a Generic Cranelift continuation.

Adding a helper, wrapper, adapter, inline fast path before an old fallback,
or a second ABI that calls the first ABI does not count as implementation.

## Forbidden in optimizing native code

- generic operation-ID helpers;
- local generic fast/slow/merge fallback blocks;
- out-pointer value helpers;
- Rust Value decode/encode for ordinary operations;
- dynamic call dispatch for a stable target;
- builtin dispatch for a prepared fixed builtin;
- local_fetch/local_store for SSA-plain locals;
- retain/release around SSA copies;
- runtime telemetry branches;
- repeated ABI, helper-ID, callsite, arity, or class validation.

## Mandatory deletion rule

When a replacement is added, the old production warm path for that operation
must be deleted in the same tranche.

Compatibility code may remain only in explicitly cold services and must not be
imported by Generic or Optimizing artifacts.

## No semantic compromise

PHP-visible checks remain mandatory:
types, references, COW, visibility, warnings, exceptions, destructors, and GC.

Engine-integrity checks happen at compilation/publication, not per invocation.

## Acceptance evidence

Source-level fast-path counters are insufficient.

### Sequencing rule

Do not run ratchets, broad gates, benchmarks, profiles, or report generation
while the native replacement is still being implemented. During the active
cutover, validation is limited to focused formatting/type-checking/builds,
native-compile probes without execution, and one semantic fixture for a
completed vertical boundary.

Collect the acceptance evidence below only after the native architecture is
integrated and the superseded production warm paths have been deleted. The
absence of fresh measurements during the incomplete cutover is not a blocker
and must not trigger an early ratchet or benchmark run.

The `PHRUST_NATIVE_CUTOVER_ACCEPTANCE=1` override exists only for that final
post-cutover acceptance run. Do not set it to bypass the implementation-time
guard. The same prohibition applies to invoking the guarded scripts directly
or reaching them through a nested `just` target; neither is a valid bypass.

Every tranche must provide:

- emitted CLIF or relocation evidence;
- forbidden-helper-import report;
- old-path deletion report;
- clean WordPress timing;
- helper/value/call/RSS deltas;
- all correctness gates.

A tranche is not complete with a 1–5% gain.
