# Request-Local Arenas And Persistent Engine Heap

Date: 2026-06-28.

FPE-19 defines the memory split needed before broader request-local arena work
can be safe. The current implementation is intentionally narrow: it records
request-local frame/register reuse as arena-like allocation evidence, exposes
fallback counters, and keeps bulk reset disabled for any allocation class that
can affect PHP-visible lifetime, references, resources, output, or destructor
order.

## Memory Worlds

### Persistent Immutable Engine Heap

Persistent engine data may survive across requests only when it is owned by the
engine and immutable for PHP-visible purposes. Valid candidates are:

- dense bytecode and rich IR metadata;
- interned strings with immutable bytes and stable target-version ownership;
- class, function, method, property, and source-map metadata;
- literal arrays only when the runtime can prove PHP-visible immutability;
- include/autoload dependency metadata and source fingerprints;
- persistent type feedback, optimized-stub descriptors, and blacklist summaries
  once FPE-20 introduces strict invalidation.

The persistent heap must not store userland `Value`s, object handles, arrays,
resources, mutable request strings, output buffers, globals, superglobals,
closures with captured request values, generators, fibers, or reference cells.
Userland object graphs are never preserved across requests.

### Request-Local Arena Or Pool

Request-local storage is eligible only when teardown is equivalent to the normal
request teardown path and PHP-visible ordering is preserved. Current safe
classes are:

- frame/register/local storage for reuse-eligible plain user-function calls;
- output root/nested buffers owned by the request;
- temporary vectors and scratch buffers whose contents cannot run destructors,
  release resources, or participate in references/COW;
- request-local runtime values, arrays, objects, resources, globals,
  superglobals, generators, and fibers only under normal per-value teardown,
  not by bulk arena reset.

## Current Implementation

The VM already keeps a request-local frame pool for reuse-eligible plain
user-function activations. FPE-19 makes that pool visible through arena-shaped
counters:

- `request_arena_allocations`
- `request_arena_bytes`
- `request_pool_resets`
- `persistent_engine_allocations`
- `persistent_engine_bytes`
- `arena_fallback_allocations_by_reason`
- `destructor_sensitive_arena_blocks`

The first implemented win is still the existing frame/register reuse path:
fresh frames increment `request_arena_allocations` and a conservative byte
estimate based on register and local slot storage; reused frames increment
`request_pool_resets`. Fallback reasons are mirrored from
`frame_reuse_blocked_by_reason` so arena analysis can separate by-reference,
closure, class-context, generator/fiber, try/finally, shared-top-level, and
destructor-sensitive blockers.

`persistent_engine_allocations` and `persistent_engine_bytes` are now populated
from the one concrete persistent immutable engine-metadata heap that already
survives across requests: the process-thread-local immutable-name interner
(`php_runtime::string::symbol_interner_footprint`). When counters are collected
the VM records the interner's footprint — the number of interned immutable names
and their total bytes — as a snapshot (set, not accumulated), so the counters
report the persistent heap size rather than a per-request delta. This heap owns
only interned immutable name bytes; it never holds userland `Value`s, handles,
arrays, resources, request strings, globals, superglobals, output buffers, or
sessions, so accounting it is safe. Broader persistent metadata classes
(compiled-unit metadata handles, source maps, validated feedback descriptors)
remain future owners of these counters.

## Blockers

Bulk arena reset is not implemented for PHP runtime values. The following
classes remain on normal owned allocations or existing runtime teardown:

- objects and arrays that can run destructors or release resources;
- references, aliases, and COW-shared containers;
- weak-reference-visible objects;
- generators and fibers with suspended frames;
- output buffers with callbacks or nested buffer ordering;
- exception, try/finally, and shutdown paths;
- include/eval/autoload state that can mutate class/function tables;
- runtime strings or buffers whose bytes can mutate or whose ownership is not
  proven immutable.

Any future request arena must prove that reset order is equivalent to normal
drop/shutdown order for every allocation class it owns.

## Safety Evidence

The frame pool refuses reuse for closure captures, by-reference parameters,
try/finally bodies, generator/fiber continuations, and object-allocation bodies
that may retain destructor-sensitive values. VM tests cover:

- call-heavy frame reuse and counter accounting;
- recursive calls;
- closure captures;
- by-reference parameters;
- try/finally output order;
- destructor order during unwind;
- generator and fiber suspension.

This is request-local storage reuse, not userland-state persistence. PHP-visible
output, diagnostics, side-effect order, reference identity, COW behavior, and
destructor timing must stay unchanged.

## Validation

Focused validation for this scope:

```bash
nix develop -c cargo test -p php_vm frame_reuse --lib
nix develop -c cargo test -p php_vm counters --lib
```

FPE-19 completion also requires the runtime and performance gates:

```bash
nix develop -c cargo test -p php_runtime -p php_vm
nix develop -c just runtime-semantics-fixtures
nix develop -c just verify-runtime
nix develop -c just verify-performance
```
