# ADR 0016: Inline Cache Invalidation Model

## Status

Accepted.

## Context

Performance introduces request-local quickening and inline-cache state for the
interpreter. `docs/performance/quickening-inline-caches.md` defines the shared
adaptive-execution model, but inline caches need a narrower invalidation
contract because their entries memoize PHP lookup results whose correctness
depends on mutable request state.

PHP lookup behavior can change when code is included or evaluated, autoloaders
are registered, class composition is finalized, properties are added, or request
configuration changes. A stale cache entry must never hide those effects.

## Decision

Inline caches are a side-table layer owned by the VM. They are keyed by the
compiled unit, function id, block id, instruction id, and cache slot id. Performance
does not encode cache entries in cached bytecode, persist them across requests,
or make them visible to optimizer passes.

Each cache entry records:

- the inline-cache kind;
- the normalized lookup key;
- one or more resolved targets;
- the relevant dependency epochs;
- the cache state: monomorphic, polymorphic, megamorphic, or disabled;
- hit, miss, invalidation, and guard-failure counters.

The baseline VM path remains the semantic source of truth. On a cache miss,
stale epoch, guard failure, unsupported shape, or disabled cache state, the VM
executes the baseline operation and preserves its output, diagnostics,
exceptions, reference behavior, COW behavior, autoload side effects, and
shutdown behavior.

## Scope

Performance inline caches may cover these lookup families:

| Cache kind | Resolved target | Required guards |
| --- | --- | --- |
| Function call IC | Function id or internal builtin descriptor. | Normalized function name, function table epoch, call shape, named argument order, variadic state, by-reference binding requirements, default-value metadata, disabled-function state. |
| Method call IC | Class id plus method slot or internal method descriptor. | Receiver class, method table epoch, visibility scope, static/instance mode, magic method absence unless explicitly represented, call shape. |
| Property fetch IC | Class id plus property slot. | Receiver class, property table epoch, visibility scope, initialized typed-property state, no magic/property-hook path, no dynamic-property shape change. |
| Class constant/static property IC | Class id plus constant or static-property slot. | Class table epoch, inheritance/composition epoch, visibility scope, initialization state, static-property storage epoch. |
| Include path IC | Resolved canonical path and include-once identity. | Include path epoch, working-directory epoch, stream-wrapper/capability epoch, require/include mode, once-set epoch. |
| Autoload/class lookup IC | Class/interface/trait/enum id or stable negative lookup. | Normalized class-like name, class table epoch, autoload stack epoch, last autoload failure epoch, include/eval declaration epoch. |

The first implementation should use the same side-table lifecycle as
quickening. Later layers may add more compact slot layouts, but they must keep
the invalidation semantics in this ADR.

The current function-call IC implementation stores the normalized function name,
function-table epoch, arity, named-argument sequence, by-reference argument
shape, and, for VM/runtime builtins, a builtin implementation id plus version.
The builtin version is request-local metadata today; future generated-arginfo or
runtime builtin replacement work must bump or change it before reusing a slot.

The current property-fetch IC implementation stores the normalized receiver
class, declaring class/property storage target, receiver class id, layout epoch,
declared property slot index, visibility context, typed-property initialized
state, property-hook presence, magic `__get` presence, and dynamic-property
fallback state. A cache hit revalidates that metadata against the request state
before returning storage; mismatches record a property IC fallback reason and
resume the baseline property path.

## Non-Goals

This ADR does not introduce inline-cache fast paths, new VM opcodes, JIT code,
cross-request cache sharing, persistent cache serialization, global worker
state, Zend ABI emulation, or standard-library behavior changes.

It also does not allow caches to skip PHP-visible behavior. Autoload invocation,
include warnings, visibility errors, magic methods, property hooks, reference
binding failures, and by-reference call errors remain baseline behavior unless
a later work item implements a guarded fast path with matching tests.

## Invalidation Events

The VM must bump the relevant epoch, invalidate affected entries, or treat
entries as stale when any of these events occur:

- function, class, interface, trait, enum, method, property, class-constant, or
  global constant declarations are added;
- include, require, require_once, include_once, or eval can declare symbols or
  alter include-once state;
- autoload callbacks are registered, unregistered, reordered, invoked, or fail
  in a way that affects future lookup;
- trait use, inheritance, interface implementation, enum case metadata, or
  class composition is finalized;
- dynamic properties are created or removed, property hooks are entered, magic
  property methods are used, or a property layout becomes non-monomorphic;
- static property storage changes shape or initialization state;
- include path, current working directory, stream wrapper registry,
  filesystem capability roots, request context, or relevant INI/config values
  change;
- disabled functions or extension availability changes inside the request.

An event may conservatively invalidate more entries than strictly necessary.
It must not leave a stale entry that can return a target the baseline lookup
would no longer return.

## Guard Failure Semantics

Guard failure is not a PHP warning or error. It is an implementation detail that
records a miss, executes the baseline operation, and optionally lowers cache
confidence.

Guards must be checked before user-visible mutations. A cache entry for an
operation that may invoke user code, allocate observable objects, throw, emit
diagnostics, or bind references should start as a resolution cache and then
tail-call the baseline executor for the visible part of the operation.

If a stale epoch is detected, the lookup is a cache miss with an invalidation
counter increment. The VM may refresh the entry from the baseline result after
the baseline operation succeeds and after any side effects are accounted for.

## Monomorphic, Polymorphic, And Megamorphic Strategy

Inline caches start cold. After repeated identical successful lookups, the VM
may install a monomorphic entry for one key/shape/target tuple.

If a small fixed number of stable shapes repeatedly occurs at the same
instruction, the entry may become polymorphic. The Performance limit is four target
arms unless a later ADR changes it. Polymorphic arms are checked in stable order
and each arm carries its own dependency epochs.

If more shapes occur, guard failures exceed the threshold, or the instruction
is dominated by stale epochs, the entry becomes megamorphic or disabled.
Megamorphic entries execute the baseline path and may keep only aggregate miss
stats. They must not keep growing unbounded target vectors.

The strategy favors under-caching over incorrect caching. It is valid for
Performance to record cache slots and stats before installing any real fast path.

## Consequences

Inline caches can remove repeated lookup work in hot interpreter paths while
keeping the generic VM operation as the correctness boundary. The cost is
explicit dependency tracking and broader invalidation tests. Documentation,
stats, and side-table slots land before fast paths so each later cache work item
has a precise fallback contract.
