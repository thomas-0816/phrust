# Phrust native hot-path replacement contract

This replaces the generic warm runtime. It does not add another
optimization layer around it.

## Non-negotiable architecture

For every operation family in scope, implementation means:

1. direct CLIF;
2. direct access to a stable native data representation;
3. a compiled native call; or
4. one native transition to a baseline continuation.

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

Compatibility code may remain only in the baseline-native tier and must not be
imported by optimizing artifacts.

## No semantic compromise

PHP-visible checks remain mandatory:
types, references, COW, visibility, warnings, exceptions, destructors, and GC.

Engine-integrity checks happen at compilation/publication, not per invocation.

## Acceptance evidence

Source-level fast-path counters are insufficient.

Every tranche must provide:

- emitted CLIF or relocation evidence;
- forbidden-helper-import report;
- old-path deletion report;
- clean WordPress timing;
- helper/value/call/RSS deltas;
- all correctness gates.

A tranche is not complete with a 1–5% gain.
