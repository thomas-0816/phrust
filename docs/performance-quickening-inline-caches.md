# Performance Quickening And Inline Caches

This document is the implementation guide for the model accepted in
`docs/adr/0074-quickening-inline-cache-model.md`. It describes how Performance
quickening and inline caches should be introduced without changing PHP
semantics.

## Execution Model

The VM starts every instruction in baseline mode. Baseline mode is the current
interpreter behavior and remains the reference implementation for every
specialized path.

Adaptive state is stored outside the IR in request-local VM state. A quickening
entry is keyed by compiled unit, function id, block id, and instruction id. The
entry records observation counters, the last stable shape, optional specialized
handler metadata, invalidation epochs, and stats.

Specialized handlers are interpreter fast paths. They may bypass generic
dispatch only after guards pass. They must call the generic path on miss or
deopt, and they must be removable without rewriting the compiled unit.

## Tiering Policy

Work item adds a request-local tiering controller above quickening, inline
caches, and the experimental JIT. The controller is intentionally small and
does not own PHP semantics. It records:

- function entry count;
- loop backedge count, counted only for backward jumps;
- inline-cache stability score from IC hits and quickening specialization hits;
- guard-failure score from quickening and IC guard failures.

The tiers are:

| Tier | Name | Policy |
| --- | --- | --- |
| 0 | Baseline interpreter | Always available and used for cold or unstable code. |
| 1 | Quickened interpreter | Considered after the function-entry or loop-backedge threshold when quickening is enabled and guard failures stay below threshold. |
| 2 | Experimental JIT | Considered only when `--jit=on`, the Cargo feature is enabled, the function is hot, and eligibility accepts the tiny int-leaf subset. |

`--tiering=off` disables adaptive quickening observations and JIT attempts for
the request, even if `--quickening=on` or `--jit=on` is also provided. The VM
still executes through the baseline interpreter. Tiering stats can be requested
without leaking into PHP stdout:

```bash
php-vm run --tiering-stats-json target/performance/tiering.json script.php
```

Configurable thresholds:

```text
--tiering-function-threshold N
--tiering-loop-threshold N
--tiering-ic-stability-threshold N
--tiering-guard-failure-threshold N
```

Counters saturate and never feed back into control-flow edges, so tiering
cannot create an execution loop. Megamorphic or guard-failing code remains in
Tier 0 for the rest of the request.

## State Shape

A quickening entry contains:

- baseline instruction identity;
- state: `cold`, `observing`, `specialized`, `megamorphic`, or `disabled`;
- execution count;
- stable-shape count;
- installed specialization kind;
- shared fallback-protocol stats;
- dependency epochs;
- stats counters.

An inline-cache entry should contain:

- cache kind, such as function call, method call, property fetch, class
  constant/static property, include path, or autoload/class lookup;
- normalized lookup key;
- resolved target or bounded polymorphic target arms;
- dependency epochs;
- state: `cold`, `monomorphic`, `polymorphic`, `megamorphic`, or `disabled`;
- hit, miss, guard-failure, invalidation, and shared fallback-protocol
  counters.

## Shared Guard/Fallback Protocol

Work item unifies quickening and inline-cache miss handling through the
`FallbackProtocolStats` structure. Every installed fast path follows the same
order:

1. run all guards before producing a value or mutating visible VM state;
2. on guard hit, record `guard_hits` and execute the specialized path;
3. on guard miss, record `guard_misses`, `guard_failures`, and exactly one
   `fallback_calls` event;
4. tail-call the baseline implementation once;
5. after the configured threshold, mark the entry dequickened/megamorphic or
   disabled for the rest of the request.

The shared protocol is applied to `ADD_INT_INT`, `CONCAT_STRING_STRING`,
`PACKED_ARRAY_INT_KEY`, and the function-call, method-call, property-fetch, and
class-constant/static-property inline-cache families. The baseline path remains
the only fallback implementation, so warnings, exceptions, magic methods,
autoload side effects, and PHP-visible conversions are not duplicated by the
fast path. `quickening-smoke` allows quickening guard failures only when they
are exactly attributed to the packed-array `numeric_string_key` fallback reason;
all such executions must still record matching guard misses and fallback calls
without dequickening the request.

## Thresholds

Initial constants:

| Name | Value | Meaning |
| --- | ---: | --- |
| Observation threshold | 8 | Start considering specialization after this many executions. |
| Stable-shape threshold | 6 | Require repeated identical shapes before installing. |
| Guard-failure threshold | 2 | Dequicken after this many failures on an installed entry. |

Counters saturate at their maximum integer value. Saturation must keep the
entry safe, not force specialization.

## Candidate Details

### `ADD` Int/Int

Guard both operands as integer values and verify the operation does not
overflow. The specialized handler may produce an integer directly. It must fall
back for floats, numeric strings, booleans, null, arrays, objects, references
that require separation, or overflow.

The implemented Performance side-table specialization is `ADD_INT_INT`. The
baseline `ADD` path still owns PHP numeric conversion: `to_number` handles null,
bool, string, and reference conversion, and checked integer overflow uses the
generic runtime diagnostic path. `ADD_INT_INT` only runs after the instruction
is hot, both runtime operands are direct `Value::Int` values, and `checked_add`
succeeds. Overflow, float operands, numeric strings, and any non-int shape
record a guard miss and tail-call the generic `ADD` path.

### `CONCAT` String/String

Guard both operands as PHP strings and use the runtime string/COW allocation
helpers. The handler must not mutate shared string storage. It must fall back
for non-string conversion, allocation failure handling, reference-sensitive
paths, or unsupported encodings. PHP string identity is not user-visible, but
later mutation behavior through COW is visible and must remain correct.

The implemented Performance side-table specialization is `CONCAT_STRING_STRING`.
It only runs for direct `Value::String` operands. The fast path preallocates one
byte buffer sized from the two `PhpString` lengths, copies both byte slices, and
returns a fresh `PhpString`. Objects with `__toString`, ints, arrays, resources,
conversion errors, and non-string shapes record a concat fast-path miss and
tail-call the generic concat path so visible conversions and diagnostics remain
unchanged.

### `FETCH_DIM` Packed Array/Int Key

Guard the receiver as an array with packed integer-key layout and the key as an
in-range non-negative integer. The handler may return the element clone through
the existing value/COW API. It must fall back for missing keys, string keys,
negative keys, mixed arrays, string offset access, object ArrayAccess, null,
warnings, or by-reference/lvalue fetches.

The implemented Performance side-table specialization is `PACKED_ARRAY_INT_KEY`.
It only runs for read-only `FETCH_DIM` instructions whose runtime operands are
direct `Value::Array` and direct non-negative `Value::Int`. The array must
still be packed at execution time according to `PhpArray::packed_metadata()`,
with keys exactly `0..len`, and the key must be in bounds. The handler returns
the element through the same effective-value path as the baseline fetch. Direct
reference elements, mixed arrays, out-of-bounds reads, numeric-string key
normalization, string offset reads, SPL/ArrayAccess-like objects, quiet fetch
behavior, warnings, and by-reference or write fetches record the relevant
packed fallback counter and tail-call the generic `FETCH_DIM` implementation.
Shared read-only arrays remain eligible because operand reads clone array
handles; mutation-sensitive append/assign paths record COW/reference fallback
before generic separation.

### `CALL` Known Function

Guard the resolved function id, function table epoch, call shape, named
argument order, variadic state, by-reference parameters, and folded defaults.
The handler may bypass name resolution but must still use the normal call frame
and argument binding logic. It must fall back for autoload/eval/include changes,
disabled functions, dynamic callables, changed defaults, by-reference errors,
or any exception-producing binding path.

The implemented Work item function-call IC caches guarded `CallFunction`
resolution for current-unit user functions, include-defined dynamic functions,
VM-managed builtins, and registered internal builtins. Each slot is keyed by
compiled unit, function, block, instruction, and IC kind. The guard checks the
normalized function name, request-local lookup epoch, arity, named-argument
sequence, by-reference argument shape, and builtin implementation id/version
when the target is a VM/runtime builtin. Static direct calls are effectively
monomorphic because their IR name is fixed. Dynamic string callables share the
same function-call IC family and grow capped polymorphic entries before the call
site switches to megamorphic fallback. A hit still tail-calls the same
user-function or builtin execution path as the baseline resolver. Includes,
eval attempts, and autoload registration changes conservatively bump the epoch;
a stale slot records an invalidation and refreshes through generic resolution.
Guard or metadata mismatches record a fallback and use the generic resolver.

The builtin intrinsic set is deliberately small and runs only while
`--inline-caches=on`: `strlen` for direct strings, `count` for direct arrays,
`is_int`, `is_string`, and `is_array` for direct non-reference values, plus
exact string intrinsics for `strtolower`, `str_contains`, `str_starts_with`,
and `str_ends_with`. Wrong arity, named-argument errors, by-reference paths,
coercion-sensitive values, and unsupported builtin shapes fall back to the
existing builtin registry path with per-builtin fallback-reason counters. The
FPE-25 ladder and deferred bytecode/native policy are documented in
`docs/performance-builtin-intrinsics.md`.

### `CALL_METHOD` Known Method

Guard the receiver class, method table epoch, normalized method name,
visibility scope, and resolved declaring class/method function. The handler may
bypass class hierarchy method lookup, but it must still validate visibility and
must still use the normal call frame and argument binding logic. It must fall
back for receiver class changes, scope changes, include/eval/autoload epoch
changes, missing methods, magic `__call`, static-as-instance errors, and any
path that would change PHP call semantics.

The implemented method-call IC supports capped polymorphic receiver entries for
successful concrete `CallMethod` resolution in current-unit classes and
include-defined dynamic classes. Each slot is keyed by compiled unit, function,
block, instruction, and IC kind. The guarded target records receiver class id,
class/method epoch, declaring method slot, visibility context, static/final/private
flags, override state, call shape, by-reference compatibility, and magic
`__call` state. Cache hits still call `validate_method_callable` and then
tail-call the same `execute_function` path as baseline dispatch. Magic `__call`
is intentionally not cached as a concrete method target; missing or inaccessible
methods continue through generic dispatch.

FPE-08 also reports metadata-only tiny-inline candidates. It does not inline
method bodies. The classifier currently accepts only final/private/final-class
leaf methods that return a scalar constant or a direct `$this` property read and
records stable rejection reasons for non-leaf, non-final/private, magic,
static, generator, reference, variadic, named/unpacked, or unavailable-body
paths. By-reference method parameters remain an existing frontend/runtime
unsupported shape, so the executable smoke fixture covers named-argument method
fallbacks and documents by-ref method fallback as a gap.

### `FETCH_PROP` Monomorphic Class Slot

Guard object class identity, class/property epoch, slot index, visibility, and
absence of hooks or magic access. The handler may read the stable slot through
the object API. It must fall back for uninitialized typed properties, dynamic
properties, readonly write-adjacent behavior, property hooks, `__get`, visibility
errors, references, or class shape changes.

The implemented property-fetch IC is monomorphic first, with a fixed-size
polymorphic receiver list capped by the shared IC limit. It is resolution-only
for declared backed property reads. It caches successful `FetchProperty`
metadata for current-unit classes and include-defined dynamic classes after the
generic path has resolved the declared property, validated visibility, skipped
hooks, read initialized storage, and proven the path is not dynamic or magic.
Each slot is keyed by compiled unit, function, block, instruction, and IC kind.
The guard checks the requested property name, normalized receiver class,
request-local lookup epoch, and, for private properties, the exact normalized
visibility scope. Public property entries use no scope guard because their
visibility is scope-independent.

Phase 09.10 makes the cached target carry explicit object/class layout metadata:
receiver class id, layout epoch, declared property slot index, visibility
context, typed-property initialized state, property-hook presence, magic
`__get` presence, and dynamic-property fallback state. Cache hits rehydrate the
declaring class and property from the owning compiled unit and still re-check
receiver class id, layout epoch, property slot, storage name, visibility, hook
metadata, active hook recursion, object storage, and uninitialized
typed-property state before returning a value. If any check fails, the hit
declines back to generic dispatch and records `property_ic_fallback_reasons`.
Dynamic properties, `__get` fallback, property hooks, protected properties, and
first reads of uninitialized typed properties are intentionally not installed as
property-cache targets.

FPE-07 adds the matching interpreter-only `AssignProperty` IC for safe declared
backed writes. The cached target carries receiver class id, layout epoch,
declared property slot, visibility context, typed-property metadata,
readonly/init-only state, reference-slot state, hook/magic/dynamic flags, and
the class/property mutation epoch. The guarded write revalidates every piece of
metadata before mutating object storage and falls back for readonly properties,
property hooks, magic `__set`, dynamic properties, typed validation failures,
reference slots, inaccessible visibility, uninitialized-special cases, and
destructor-sensitive generic behavior. Dense bytecode still treats property
assignment as an unsupported/auto-fallback family until the rich interpreter
write helper is reusable from dense execution. Object-property references are
also rejected during lowering today, so `reference_slot` assignment exits are
tracked in counters but not yet reachable from an executable PHP fixture.

### `FETCH_CLASS_CONST` And Static Property Metadata

The implemented Work item class/static IC is monomorphic and
resolution-only. It covers class constants, enum cases, and static-property
metadata under the shared `ClassConstantStaticProperty` IC family. Each slot is
guarded by resolved class name, member name, cache sub-kind, request-local
lookup epoch, and exact normalized visibility scope when the member is private
or protected. Public members use no scope guard.

Class-constant hits rehydrate the declaring class and constant, validate
visibility again, and evaluate the constant through the owning compiled unit.
Enum-case hits rehydrate the enum case and still use the existing enum-case
object table. Static-property hits rehydrate only metadata, validate visibility,
initialize default storage if needed, and read the current value from
`state.static_properties`; the IC never stores mutable static-property values.
Static-property writes remain on the generic path.

## Invalidation

Each quickening or inline-cache entry records the epochs it depends on. A stale
epoch turns the operation into a miss and either refreshes observation or
dequickens the entry.

Inline-cache invalidation is specified by
`docs/adr/0075-cache-invalidation-model.md`. Performance cache entries are
request-local VM side-table entries. They are not serialized into bytecode
cache artifacts and are not optimizer inputs.

Inline-cache families:

| Cache kind | Baseline operation | Guarded dependency shape |
| --- | --- | --- |
| Function call IC | Function lookup plus argument binding. | Normalized function name, function table epoch, resolved function id or builtin descriptor, call shape, named/variadic/by-ref/default metadata. |
| Method call IC | Receiver/class method lookup plus call binding. | Receiver class id, class/method epoch, method slot, visibility scope, static/final/private/override state, call shape, by-reference compatibility, magic-method state. |
| Property fetch IC | Generic property read. | Receiver class id, property table epoch, property slot, visibility scope, initialized typed-property state, no dynamic-property, magic, or hook path. |
| Property assignment IC | Generic property write. | Receiver class id, property table epoch, property slot, visibility scope, typed-property metadata, readonly/init-only state, no dynamic-property, magic, hook, reference, or inaccessible path. |
| Class constant/static property IC | Class constant or static-property lookup. | Class table epoch, class-composition epoch, slot id, visibility scope, initialization/storage epoch. |
| Include path IC | Include/require path resolution and once checks. | Include path epoch, working-directory epoch, stream-wrapper/capability epoch, canonical path, include-once set epoch. |
| Autoload/class lookup IC | Class/interface/trait/enum lookup and autoload. | Normalized class-like name, class table epoch, autoload stack epoch, declaration epoch, stable positive or negative lookup result. |

Events that must bump or invalidate relevant epochs:

- include, require, require_once, include_once, or eval that declares symbols;
- function/class/method/property/class-constant/enum additions;
- autoload stack registration, unregistration, invocation, or failed lookup;
- trait use, inheritance, interface implementation, enum case, or class
  composition finalization;
- include path or working directory changes;
- request context or INI changes that affect lookup behavior;
- class table, method table, property table, static property, or constant table
  mutations;
- dynamic property creation for a class previously assumed monomorphic;
- property hook or magic-property shape changes;
- filesystem capability, stream wrapper, disabled-function, or extension
  availability changes that affect lookup.

Performance does not persist quickening state globally. Cross-request or shared
worker state needs a later lifecycle design.

An invalidation event may conservatively clear a whole cache family. It must
not leave an entry that can return a target the baseline lookup would no longer
return.

## Inline-Cache Shape Strategy

Inline caches start cold. After the same key and guarded dependency shape
repeats, the VM may install a monomorphic entry. Monomorphic entries have one
target and one dependency-epoch set.

If a small fixed number of stable shapes occurs at the same instruction, the
entry may become polymorphic. Performance caps polymorphic entries at four arms.
Each arm is checked independently and carries its own dependency epochs.

When an instruction sees too many shapes, repeated guard failures, or frequent
stale epochs, the entry becomes megamorphic or disabled. Megamorphic entries
execute the baseline path and keep only bounded aggregate stats. They must not
grow target vectors without a fixed limit.

## Guard Failure And Dequickening

Guard failure increments shared protocol stats and executes the baseline
instruction exactly once. It is not a PHP warning or error. After the
guard-failure threshold, quickened instructions dequicken to `megamorphic` and
clear their installed specialization. Inline-cache slots first report a
megamorphic transition for shape instability and become `disabled` after the
disabled threshold; disabled slots execute baseline resolution and skip target
reinstall for the rest of the request.

Dequickening must be deterministic. A program must produce the same observable
behavior whether the failure happens before or after specialization.

Specialized handlers must check all guards before making visible mutations. For
operations that can invoke user code, throw, allocate objects, or trigger
destructors, the specialized path should start as resolution-only and then tail
call into the baseline operation.

## Stats And Counters

Quickening and inline-cache stats should be exposed through optional VM counters
only when counter collection is enabled. Current fields:

| Field | Meaning |
| --- | --- |
| `quickening_attempts` | Baseline dispatch observations collected for adaptive entries. |
| `quickening_specialized` | Metadata or concrete specialization installs recorded. |
| `quickening_guard_hits` | Installed specialized handlers whose guards passed. |
| `quickening_guard_misses` | Installed specialized handlers that fell back before producing a value. |
| `quickening_guard_failures` | Failed guards on installed entries. |
| `quickening_fallback_calls` | Shared-protocol fallback calls that tail-called the generic operation. |
| `quickening_dequickens` | Entries dequickened or disabled. |
| `quickening_megamorphic` | Quickening entries that transitioned to megamorphic baseline-only state. |
| `quickening_disabled` | Quickening entries disabled for the request. |
| `string_concat_fast_path_hits` | `CONCAT_STRING_STRING` handler executions whose guards passed. |
| `string_concat_fast_path_misses` | `CONCAT_STRING_STRING` entries that fell back to generic concat. |
| `packed_dim_fast_path_hits` | `PACKED_ARRAY_INT_KEY` handler executions whose guards passed. |
| `packed_dim_fast_path_misses` | `PACKED_ARRAY_INT_KEY` entries that fell back to generic dim fetch. |
| `packed_fetch_fast_hits` | Packed-array int-index fetches completed by the guarded interpreter/JIT path. |
| `packed_fetch_bounds_fallbacks` | Installed packed-fetch guards that failed because the integer index was out of bounds. |
| `packed_fetch_layout_fallbacks` | Installed packed-fetch guards that failed because the receiver/key was not an eligible packed-list int-index read. |
| `packed_append_fast_hits` | Packed append writes that stayed on the guarded packed path without COW/reference fallback. |
| `packed_foreach_fast_hits` | By-value packed-list foreach snapshots that were reference-free and used the packed fast path. |
| `cow_or_reference_fallbacks` | Array fast paths that stayed generic because COW separation or direct reference elements were present. |
| `inline_cache_observations` | Candidate IC instruction observations while inline caches are enabled. |
| `inline_cache_slots` | Request-local IC slots allocated for candidate instructions. |
| `inline_cache_function_slots` | Function-call IC slots allocated. |
| `inline_cache_method_slots` | Method-call IC slots allocated. |
| `inline_cache_property_slots` | Property-fetch IC slots allocated. |
| `inline_cache_property_assign_slots` | Property-assignment IC slots allocated. |
| `inline_cache_dim_slots` | Dimension-fetch IC slots allocated. |
| `inline_cache_class_constant_static_property_slots` | Class-constant/static-property IC slots allocated. |
| `inline_cache_include_path_slots` | Include-path IC slots allocated. |
| `inline_cache_autoload_class_lookup_slots` | Autoload/class-lookup IC slots allocated. |
| `inline_cache_hits` | Lookup cache hits. |
| `inline_cache_misses` | Lookup cache misses. |
| `inline_cache_invalidations` | Lookup cache misses caused by stale dependency epochs. |
| `inline_cache_guard_failures` | IC guards that failed before a fast path could produce a value. |
| `inline_cache_fallback_calls` | Shared-protocol IC misses that used baseline lookup/dispatch. |
| `inline_cache_megamorphic` | IC slots that transitioned to megamorphic state. |
| `inline_cache_disabled` | IC slots disabled after repeated shape-changing guard failures. |
| `function_call_ic_hits` | Function-call IC guard hits. |
| `function_call_ic_misses` | Function-call IC misses, including cold slots and guard failures. |
| `builtin_call_ic_hits` | Function-call IC hits whose cached target is a VM/runtime builtin. |
| `builtin_call_ic_misses` | Function-call IC misses that resolve to a VM/runtime builtin through fallback. |
| `builtin_fast_stub_hits` | Per-builtin hits for the small exact fast-stub set. |
| `builtin_fast_stub_misses` | Per-builtin misses that fell back to the generic builtin path. |
| `builtin_fast_stub_fallback_by_reason` | Per-builtin fallback reasons for exact fast stubs, keyed as `name.reason`. |
| `builtin_intrinsic_candidates` | Supported builtin intrinsic calls considered while inline caches are enabled. |
| `intrinsic_hits` | Per-intrinsic exact fast-path hits. |
| `intrinsic_misses` | Per-intrinsic guard misses that fell back to generic builtin execution. |
| `intrinsic_fallback_by_reason` | Per-intrinsic fallback reasons keyed as `name.reason`. |
| `specialized_builtin_opcode_hits` | Future specialized bytecode builtin opcode hits; empty until bytecode parity fixtures exist. |
| `call_ic_megamorphic_fallbacks` | Function-call IC sites that reached megamorphic fallback state. |
| `method_ic_hits` | Method-call IC guard hits. |
| `method_ic_misses` | Method-call IC misses, including cold slots and guard failures. |
| `method_ic_polymorphic_hits` | Method-call IC hits served by capped polymorphic receiver entries. |
| `method_ic_guard_failures` | Method-call IC misses caused by receiver, method, or scope guard failure. |
| `method_direct_dispatch_hits` | Cached method targets dispatched through the existing VM method-call helper. |
| `method_direct_dispatch_fallbacks` | Method-call IC fallback observations or guarded cached targets rejected before direct dispatch. |
| `method_tiny_inline_candidates` | Metadata-only count of tiny-safe method bodies that could be considered by a future inliner. |
| `method_tiny_inline_rejected_by_reason` | Stable rejection reason map for metadata-only tiny inlining classification. |
| `property_ic_hits` | Property-fetch IC guard hits. |
| `property_ic_misses` | Property-fetch IC misses, including cold slots and guard failures. |
| `property_ic_guard_failures` | Property-fetch IC misses caused by receiver, property, or scope guard failure. |
| `property_ic_fallback_reasons` | Per-reason slow-path exits when a cached property target fails layout/storage/visibility revalidation. |
| `property_assign_ic_hits` | Property-assignment IC guard hits that completed a guarded declared-property write. |
| `property_assign_ic_misses` | Property-assignment IC misses, including cold slots and guard failures. |
| `property_assign_ic_guard_failures` | Property-assignment IC misses caused by receiver, property, scope, or write-policy guard failure. |
| `property_assign_ic_shape_exits` | Property-assignment exits caused by receiver class, layout epoch, slot, storage-name, or metadata mismatch. |
| `property_assign_ic_visibility_exits` | Property-assignment exits caused by visibility or setter-visibility mismatch. |
| `property_assign_ic_type_exits` | Property-assignment exits caused by typed-property validation failure. |
| `property_assign_ic_readonly_exits` | Property-assignment exits caused by readonly or already-initialized readonly state. |
| `property_assign_ic_hook_magic_exits` | Property-assignment exits caused by property hooks or magic `__set`. |
| `property_assign_ic_reference_exits` | Property-assignment exits caused by reference-bearing object storage. |
| `property_assign_ic_dynamic_exits` | Property-assignment exits caused by dynamic-property fallback. |
| `property_assign_ic_fallback_reasons` | Per-reason slow-path exits for guarded property-assignment writes. |
| `class_static_ic_hits` | Class-constant/static-property IC guard hits. |
| `class_static_ic_misses` | Class-constant/static-property IC misses, including cold slots and guard failures. |
| `class_static_ic_guard_failures` | Class-constant/static-property IC misses caused by class, member, kind, or scope guard failure. |

Counters are evidence and debugging aids. They must not control PHP-visible
behavior unless the work item explicitly introduces adaptive thresholds tied to
those counters.

## Validation Requirements

Each specialization work item must include:

- baseline/quickened A/B fixture coverage;
- at least one guard-hit test;
- at least one guard-failure fallback test;
- a reference/COW/reference-sensitive negative test where relevant;
- stats assertions proving hits and misses;
- `nix develop -c just verify-performance`.

The initial framework work item kept specialization conservative while installing
the state machine and stats. Current Performance smoke gates exercise concrete
quickening and inline-cache specializations, guard hits, guard misses, fallback
calls, disabled/megamorphic transitions, and epoch invalidation counters.

## Developer Commands

Run the adaptive smoke gates:

```bash
nix develop -c just quickening-smoke
nix develop -c just inline-cache-smoke
nix develop -c just polymorphic-inline-cache-smoke
nix develop -c just perf-flag-matrix
nix develop -c just verify-performance
```

Inspect counters for a single fixture:

```bash
nix develop -c cargo build -p php_vm_cli --bin php-vm
nix develop -c target/debug/php-vm run \
  --quickening=on \
  --inline-caches=on \
  --counters-json target/performance/adaptive-counters.json \
  tests/fixtures/performance/perf_smoke/function_calls.php
```

Inspect tiering decisions without writing to PHP stdout:

```bash
nix develop -c target/debug/php-vm run \
  --quickening=on \
  --inline-caches=on \
  --tiering-stats-json target/performance/tiering.json \
  tests/fixtures/performance/perf_smoke/function_calls.php
```

Disable tiering for a baseline comparison:

```bash
nix develop -c target/debug/php-vm run \
  --quickening=on \
  --tiering=off \
  tests/fixtures/performance/perf_smoke/arithmetic.php
```

## Troubleshooting

- Output differences: rerun the same fixture with `--quickening=off` and
  `--inline-caches=off`, then inspect `target/performance/quickening-smoke` or
  `target/performance/inline-cache-smoke` artifacts for the first differing mode.
- Missing hits: check that the fixture is hot enough for thresholds and that
  `--tiering=off` was not used accidentally.
- Guard-failure loops: counters should show fallback/dequickening and then
  baseline execution. If counters grow without disabling a bad entry, fix the
  state machine before expanding the specialization.
- Stale IC result: look for missing epoch bumps around include, eval, autoload,
  class/function registration, property metadata, or static-property metadata.
