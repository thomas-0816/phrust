# Steps: Correct Native Hot-Path Cutover

This plan turns the outcome in `goal.md` into a sequence of large vertical
cuts. Progress is measured by the real execution path, not by deleted symbol
counts or source-level fast-path counters.

The current state is not a completed cutover. The latest WordPress profile
records 39,969 baseline entries, 39,968 same-unit transitions, zero optimizing
entries, 100% baseline hot time, and 574,019 runtime-helper calls. The source
has partially isolated optimizing and baseline capabilities, but publication
routes the complete WordPress working set through fragmented baseline
artifacts.

## Measurement sequencing

Do not return to continuous ratchets, broad benchmark suites, or per-edit
profiling. However, a completed large vertical cut must receive one fixed,
response-identical WordPress A/B canary before the next cut begins. Otherwise
the implementation can remain structurally compliant while the actual hot
path deteriorates unseen.

The canary is evidence, not a source-level proxy:

- use the same Nix inputs, WordPress tree, database snapshot, host, and
  configuration;
- require identical HTTP status, body bytes, and body hash before interpreting
  performance;
- compare against an immutable last-known-correct Phrust build and the PHP
  oracle;
- do not advance when a cut has merely moved work into baseline fragments;
- reserve broad gates, ratchets, reports, and the bounded benchmark matrix for
  final acceptance.

## Step 0: Establish an honest starting point

Make the current state correct and reproducible before changing another
architecture boundary.

- Fix the current `count(uninitialized)` and stack-overflow failure generically.
- Require PHP and Phrust to return the same HTTP status, body, and body hash.
- Pin the WordPress tree, database identity, host configuration, and Nix
  derivations used for comparison.
- Preserve reproducible builds of the last known correct Phrust checkpoint and
  the current checkpoint.
- Separate the current semantic fixes from the compile-fragment splitter so
  they are not presented as one performance change.

Completion means that the real WordPress request is correct and reproducible.
It does not imply that the request is fast.

## Step 1: Complete the call, by-reference, and publication cutover

This is the first major performance boundary. The five largest measured helper
families -- builtin calls, function calls, dynamic code, method calls, and
reference binding -- account for roughly 96 of 102 measured helper seconds.

Publish one complete native contract for every admitted callsite:

- callable kind and stable target;
- receiver or closure shape;
- complete signature, arity, and defaults;
- by-reference parameter and return map;
- variadic and unpack lengths and shapes;
- callback argument-array layout;
- argument, frame, return-slot, cleanup, and ownership plans;
- typed return, warning, exception, and runtime-error outcomes.

Use the published contract to:

- pass native encodings unchanged through arguments, frames, variadics, and
  return slots;
- bind by-reference arguments through trusted numeric lvalue plans;
- traverse native arrays directly for unpack and callback argument arrays;
- directly call stable functions, methods, callbacks, and prepared builtins;
- route an unprovable call once to baseline before a coarse optimizing region;
- form whole functions or large natural regions instead of one fragment per
  call or instruction.

Delete or baseline-isolate in the same cut:

- optimizing use of `lower_native_call_trampoline`;
- generic function, method, and builtin dispatch;
- generic reference binding at admitted callsites;
- callframe demote/promote cycles;
- same-unit fragments created solely around calls.

Completion evidence:

- WordPress remains response-identical;
- WordPress executes real optimizing entries;
- call and reference families no longer dominate inclusive execution time;
- same-unit transitions no longer scale with the number of calls;
- optimizing artifacts import no generic call or builtin dispatcher.

## Step 2: Close the native storage, lvalue, COW, and ownership plane

Preserve the same authoritative native identity through every common storage
boundary:

- locals, arguments, and returns;
- globals and static locals;
- direct and nested array entries;
- declared and static properties;
- array and property lvalues;
- reference payloads and reference returns;
- COW writeback;
- temporary SSA owners.

Publication must determine:

- the native tag and payload compatibility;
- initialization and lifetime;
- ownership transfer, retain, move, and release;
- reference identity and supported cycle state;
- array uniqueness, COW state, and output capacity;
- property layout, numeric slot, visibility, and magic-method state.

Admitted execution then reads or updates the authoritative native slot once.
It must not implement a mutation as fetch, bind, materialize, mutate, publish,
and release through Rust `Value`.

Delete or baseline-isolate in the same cut:

- optimizing `local_fetch` and `local_store` for SSA-plain locals;
- generic array fetch, insert, unset, and writeback for admitted shapes;
- generic reference binding and `ReferenceCell` materialization;
- property-name hashing for trusted declared slots;
- copied-value property caches;
- native-to-Rust-to-native mirror synchronization.

Completion evidence:

- calls, arrays, properties, references, and returns preserve native identity;
- `array_fetch`, `array_insert`, and `reference_bind` are no longer warm helper
  hotspots;
- reference, COW, visibility, destructor, and GC fixtures match reference PHP;
- the optimizing tier cannot import the removed storage compatibility paths.

## Step 3: Close frequent builtin, string, array, and scalar families

Migrate complete representation families rather than individual builtin names:

- string length, comparison, search, and transformation;
- array key, shape, count, lookup, projection, and callback families;
- type, resource, capability, and configuration queries;
- scalar comparison, cast, arithmetic, bit, shift, and truthiness operations.

For each family:

- classify operand types, shapes, bounds, capacities, and semantic outcomes at
  publication;
- emit direct CLIF for representation-simple operations;
- use a total native call for representation-heavy operations;
- return only a native value, typed throw/runtime error, or `ABI_MISMATCH` for
  a violated publication contract;
- reject unprovable forms before optimizer entry;
- never enter generic prepared-builtin dispatch or return
  `RECOMPILE_REQUESTED` from an admitted exact handler.

Delete the superseded generic warm path for each completed family in the same
cut.

Completion evidence:

- prepared fixed builtins import no baseline builtin dispatcher;
- scalar and truthiness lowering contains no local generic slow/merge island;
- native string and array results remain native across subsequent storage and
  call boundaries;
- runtime-helper boundaries meet or materially approach the final limit of
  100,000 without moving the work into another generic helper.

## Step 4: Remove baseline fragmentation from the hot tier

Once calls, storage, and common builtins are representation-complete, remove
the fragmented baseline execution architecture from ordinary execution.

- Publish whole hot functions or a small number of natural regions.
- Do not downgrade a complete hot function merely because baseline helper
  imports exist elsewhere in its unit.
- Give a rare unsupported semantic shape one outer baseline continuation.
- Make `lower_baseline_region_instruction` reachable only while compiling a
  genuine baseline/cold artifact.
- Make baseline operation tables and helper addresses unrepresentable in an
  optimizing artifact.
- Remove exact pre-regalloc replanning and splitting that explodes one
  function into thousands of CLIF blocks and megabytes of code.
- Eliminate ordinary same-unit transition chains.

Completion evidence:

- at least 95% of inclusive hot execution uses optimizing entries;
- ordinary operation-local transitions are zero;
- baseline execution is observably cold rather than the renamed warm path;
- code size and region count are bounded by functions and natural semantic
  boundaries, not instruction count.

## Step 5: Isolate cold state and amortize compilation

Physically finish the native/cold separation:

- keep `NativeRequestColdState`, Rust `Value`, decode, encode, and
  materialization exclusively in baseline/cold modules;
- prevent recovery of cold state from a fast-state pointer in optimizing code;
- shrink common ABI modules to stable data types and native contracts;
- remove identity maps and wrappers used only to synchronize two value planes;
- compile and publish hot functions once instead of compiling thousands of
  request-synchronous fragments;
- reuse published native artifacts across requests;
- finish bounded request, frame, value-slot, and arena pooling after the
  authoritative representation is complete.

Completion evidence:

- forbidden-import and relocation reports prove that optimizing artifacts
  cannot reach cold value functionality;
- WordPress no longer performs thousands of fragment compilations per request;
- warm execution is not dominated by compilation, allocation, or code size;
- old value allocations and RSS remain within the limits in `goal.md`.

## Step 6: Perform final `goal.md` acceptance

Only after the native architecture and mandatory deletion are complete, run
the broad correctness, benchmark, profile, report, and ratchet suite.

Required evidence:

- full PHP correctness gates;
- emitted CLIF and relocation evidence;
- forbidden-helper-import report;
- old-path deletion report;
- response-identical clean and diagnostic WordPress runs;
- at least 95% optimizing inclusive hot execution;
- zero ordinary operation-local transitions;
- warm WordPress c1 p50 at most 80 ms and p95 at most 100 ms;
- at most 100,000 runtime-helper boundaries;
- at most 10,000 old value allocations;
- c1 peak RSS at most 300 MB and c8 RSS at most 500 MB on the same host and
  configuration.

After these conditions pass, complete request pooling, the bounded measurement
matrix, the minimal breakthrough gate, and final ratchet updates required by
`hotpack.md`.

## Required order

```text
correctness and reproducibility
    -> calls, frames, references, and publication
    -> storage, lvalues, COW, and ownership
    -> builtin, string, array, and scalar families
    -> removal of baseline fragmentation
    -> physical cold-state isolation and compilation reuse
    -> full acceptance
```

Do not begin a later step to hide a failure in an earlier one. In particular,
do not delete more fallback symbols before the corresponding real WordPress
boundary executes through its replacement.
