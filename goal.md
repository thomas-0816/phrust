# Goal: Authoritative Native Value, Call, and Storage Cutover

## Outcome

Complete the native hot-path replacement described by `AGENTS.md` and
`/data/src/ml/phrust/hotpack.md` without returning to operation-by-operation
fast paths.

The end state is:

> Optimizing code produces, stores, passes, returns, and consumes only native
> encodings backed by authoritative native slots. Rust `Value` exists only
> behind an explicit baseline/cold, extension, reflection/debug, or final
> outer-result boundary.

This is one coordinated vertical cutover. Introducing a representation and
leaving the old common path available is not an intermediate completion.

## Current starting point

The starting source checkpoint is commit `8793aff4` on `main`.

Already present and not to be rebuilt:

- a real `NativeRequestFastState`, separate `NativeRequestColdState`, and
  request owner;
- stable demand-backed native arenas;
- direct native value, string, array, object, property, and reference records;
- executable SSA ownership and last-use analysis;
- trusted native call entries and prepared metadata;
- typed native control results and a growing set of exact native handlers;
- direct array, property, static, global, and reference operations.

The remaining shared defect is the dual value plane. Common execution can
still cross between:

```text
native encoding / JitNativeValueSlot
    -> decode or reference demotion
Rust Value / NativeStoredValue::Php
    -> encode or promotion
native encoding / JitNativeValueSlot
```

This occurs across call argument binding, variadics and unpacking, reference
reads and writes, properties and other storage, and remaining builtin paths.
It causes repeated graph materialization and keeps the complete cold runtime
reachable from otherwise native execution.

## Binding invariant

For every common optimizing producer, storage boundary, call, return, and
consumer:

1. The value is represented by an encoded native value or a numeric native
   lvalue location.
2. Ownership is transferred, retained, or released exactly once according to
   `ExecutableValueFlow` and the native slot/header state.
3. Arrays, strings, objects, and references preserve their native identity and
   representation across the boundary.
4. PHP-visible type, reference, COW, visibility, warning, exception,
   destructor, and GC semantics are preserved.
5. An unsupported semantic shape takes one exact baseline continuation. It
   does not enter a local fast/slow helper chain.

Ordinary optimizing execution must not call or depend on:

- `decode()`, `decode_result()`, or Rust `Value` construction;
- `encode()`, `encode_stored_value()`, or mirror-store synchronization;
- `NativeStoredValue::Php`;
- `duplicate_native_call_argument()` or a demote/promote cycle;
- recovery of `NativeRequestColdState` from the fast-state pointer;
- generic prepared-builtin dispatch;
- stringly typed control parsing;
- property-name hashing or copied-value property caches for trusted declared
  properties;
- operation-local generic fallback blocks.

## One vertical implementation tranche

### 1. Make calls and frames native end to end

Carry native encodings unchanged through userland and exact builtin calls.

- Store native encodings in call frames, locals, argument plans, variadics,
  and return slots.
- Transfer argument and result ownership from the existing SSA analysis.
- Bind by-value arguments without demoting references through a Rust
  `ReferenceCell`.
- Bind by-reference arguments through a trusted numeric lvalue plan.
- Traverse direct arrays for argument unpacking and `call_user_func_array`.
- Preserve native array, string, object, and reference values across unit and
  function boundaries.
- Return the native encoding directly; do not reconstruct a `Value` tree.

The scope includes the shared paths in `jit_abi.rs`, `call_support.rs`, and
`call_dispatch.rs`. Do not solve this by adding another call adapter.

### 2. Make references and lvalues native end to end

Use the existing native reference views and executable ownership facts to
prepare direct lvalue plans for:

- locals and local aliases;
- globals and static locals;
- declared and static properties;
- array dimensions and nested dimensions;
- call by-reference arguments and reference returns;
- COW writeback.

The admitted path reads or updates the authoritative native payload once. It
must not lower to a sequence of fetch, bind, materialize, mutate, publish, and
release operations through Rust `Value`.

Cold `ReferenceCell` materialization may remain only after an explicit
baseline/cold transition. It must not be converted back and forth during
ordinary native execution.

### 3. Make storage preserve native values

Carry the same native representation through:

- direct array entries and nested arrays;
- globals and static locals;
- declared and static property slots;
- function and method arguments;
- returns and temporary SSA owners.

Trusted declared properties use numeric layout/slot records and actual native
slot storage. Ordinary access must not hash a property name, clone a Rust
`Value`, or probe a copied-value cache.

Direct strings remain native through concatenation, comparison, array/property
storage, calls, returns, builtins, and output.

### 4. Move complete builtin families to exact handlers

Prepared fixed builtins use their already published exact handler metadata and
typed `JitNativeControlResult`.

- Pure handlers receive native values only.
- Capability handlers receive only the explicitly published capability.
- Native string and array results remain native.
- A rare unsupported shape takes one exact cold operation or baseline
  continuation.
- Optimizing code never enters the baseline prepared-builtin dispatcher.
- Exact handlers do not parse `E_PHP_*` strings for ordinary control.

Migrate representation-complete families, not isolated builtin names. Start
with the shared string, array, type/query, count/length, key, and callback
argument boundaries already reached by native code.

### 5. Delete the superseded common path in the same tranche

After all scoped producers and consumers use native values, delete or make
unambiguously baseline/cold-only:

- common `decode`/`encode` call paths;
- common `NativeStoredValue::Php` allocation and lookup;
- direct-reference demotion/promotion cycles and their temporary telemetry;
- common identity maps used only to synchronize two value planes;
- compatibility array/object/string reconstruction at native call boundaries;
- copied-value declared-property caches;
- optimizing generic prepared-builtin routing;
- stringly control conversion for exact handlers;
- operation-local fallback blocks made obsolete by the native representation;
- wrappers whose only purpose is to keep the old warm implementation alive.

Keep cold PHP semantics that are genuinely needed. Rename or move them so
their baseline/cold role is explicit and they cannot be imported by optimizing
artifacts.

## Working method

Do not use a sequence of callsite micro-optimizations to pursue this goal.

- Work from the shared representation boundary across all of its producers,
  storage locations, calls, returns, and consumers.
- Do not add per-callsite telemetry, a new ratchet, a new view, or another
  compatibility facade while the cutover is in progress.
- Do not run a multi-minute WordPress request after each edit.
- During implementation, use formatting, type checking, and at most one
  focused semantic check when needed for immediate feedback.
- It is acceptable for the uncommitted tranche to be temporarily incomplete.
- Restore correctness by completing the native representation, not by
  reconnecting the old common runtime.
- WordPress is an external, application-neutral acceptance workload. Never
  specialize Phrust for WordPress names, classes, files, callsites, or data.

## Tranche checkpoint

Run the checkpoint only after the complete shared boundary has been migrated
and its old common path has been deleted or baseline-isolated.

The checkpoint must prove:

- formatting and compilation succeed;
- focused call/reference/COW/property/destructor semantics match reference
  PHP;
- optimizing artifacts import no generic helper, generic builtin dispatcher,
  cold-context recovery, or common conversion operation;
- calls, returns, arrays, strings, references, and properties remain native
  across the tested boundary;
- the relevant old production symbols and paths are gone;
- one clean and one separate diagnostic WordPress request are response-
  identical and complete without a crash or stale-handle failure.

Do not interpret a small counter reduction or a 1-5% latency change as tranche
completion. If the integrated request remains slow, identify the next largest
shared representation boundary and replace it as another complete vertical
tranche.

## Full completion conditions

This goal is complete only when the complete `hotpack.md` contract is proven,
including:

- native values are authoritative throughout common execution;
- the old Rust value plane is cold-only;
- common calls, returns, arrays, foreach, strings, properties, references,
  COW, lvalues, and fixed builtins remain native;
- ordinary operation-local transitions are zero;
- at least 95% of inclusive hot execution uses optimizing entries;
- generic runtime helpers remain baseline/cold-only;
- the deletion contract identifies every superseded common path;
- the full correctness gates pass;
- the integrated clean and diagnostic artifacts exist;
- warm WordPress c1 p50 is at most 80 ms and p95 at most 100 ms;
- runtime-helper boundaries are at most 100,000;
- old value allocations are at most 10,000;
- c1 peak RSS is at most 300 MB and c8 RSS is at most 500 MB on the same
  host/configuration.

After the authoritative cutover and deletion are complete, finish request
pooling, the bounded measurement matrix, the minimal breakthrough gate, and
the final ratchet updates required by `hotpack.md`. These must not delay or
fragment the native replacement.
