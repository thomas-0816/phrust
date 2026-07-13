# Cranelift-only cutover

ADR 0017 makes executable Region IR lowered by Cranelift the sole production
execution contract. The migration is sequential: each prompt must pass its own
gate and shrink the temporary source allowlist before the next prompt begins.

## Prompt 0 baseline (historical)

The pre-cutover revision was
`c300e22a5f389c1e6b022f40184e79c9980e8cd7`. Its detached comparison harness
was used only while native coverage was being established. Prompt 11 removed
that harness together with the old executor. Current differential checks invoke
the pinned external PHP 8.5.7 reference binary only.

`scripts/verify/cranelift_only_allowlist.json` is the temporary legacy-source
inventory. New alternate-executor references are rejected immediately. A path
was removed from the allowlist in the same prompt that removed its last legacy
reference. Both executor categories are empty as of Prompt 11.

## Prompt 1 alternate-emitter removal

The former handwritten native emitter and its generated stencil artifacts are
deleted. `nix develop -c just cranelift-only-no-alternate-emitter` rejects any
new reference to that implementation and builds both product binaries through
the Cranelift dependency graph.

## Prompt 2 mandatory compiler boundary

Cranelift is a non-optional dependency and there is no product backend or
native-off selector. The two supported profiles are:

- `baseline`: eager, non-speculative Cranelift compilation with adaptive
  optimization disabled;
- `default` (and its `fast` alias): optimizing Cranelift compilation.

`Vm::execute` is the product native entry boundary. Until later prompts make
lowering exhaustive, an unsupported entry fails setup with
`E_NATIVE_UNSUPPORTED_LOWERING`, the precise IR `InstructionKind`, and its
byte span. It never resumes through the retained in-crate test oracle.

Run the mandatory graph and release-symbol gate with:

```sh
nix develop -c just cranelift-only-mandatory
```

The gate checks every product binary's Cargo graph, builds the release server,
requires Cranelift compiler symbols, and rejects retired emitter or interpreter
entry symbols in that binary.

## Prompt 3 authoritative compiler pipeline

`BaselineRegionBuilder` constructs a complete multi-block `RegionGraph`
directly from `php_ir::IrFunction`. The graph retains the source operation and
span even when Cranelift coverage is missing, so unsupported semantics produce
a stable missing-lowering error instead of selecting another executor.

Methods, closures, generators, by-reference declarations, strict-types mode,
captures, declaration identity, and function attributes all enter this same
pipeline. Native ABI gaps may still prevent publication, but function shape no
longer prevents graph construction. `JitEngine::compile_unit` creates a
baseline record for every known body before the VM publishes the entry point.

Run the authoritative-IR gate with:

```sh
nix develop -c just cranelift-baseline-ir-coverage
```

Native code cache identity includes the IR fingerprint, compiler tier,
runtime/helper ABI identity, target CPU, semantic configuration, dependency
identity, and invalidation generation.

## Prompt 4 exhaustive lowering inventory

All 102 `InstructionKind` variants and all six `TerminatorKind` variants are
classified by exhaustive Rust matches generated from one typed manifest list.
The nested unary, binary, comparison, cast, include, callable, and call-argument
enums also have wildcard-free classifiers. Adding a variant therefore fails to
compile until its lowering class and effect metadata are supplied.

The gate writes the generated reports to
`target/cranelift-only/instruction-coverage.json` and
`target/cranelift-only/instruction-coverage.md`:

```sh
nix develop -c just cranelift-exhaustive-lowering
```

`RuntimeError` is represented as a native fatal status. `Unsupported` remains
an explicit compile fatal during the staged migration and must be absent from
reachable production IR by the final cutover.

## Prompt 5 typed runtime operations

The stable `JitHelperId` type is owned by `php_runtime` and shared by both
Cranelift tiers. The runtime-operation registry records a versioned signature,
typed operands and result/status, ownership, concrete implementation, callers,
PHP-visible effects, and safepoint requirements for every required semantic
family. Native-callable entries must identify a real backend-neutral
`php_runtime::api::native_*` operation and be callable from both baseline and
optimizing code.

Unary, binary, comparison, cast, and echo semantics are the first IR operations
mapped to this ABI. Their implementations were extracted from VM scalar
dispatch into `php_runtime`; the VM now consumes the same operations. Results
use caller-owned `Value` slots, failures use explicit statuses and request
diagnostic state, operands are compiler-independent enums, and the operation
module has no dependency on VM or IR dispatch types. The native cache key folds
in the runtime-operation ABI hash.

Run the gate with:

```sh
nix develop -c just cranelift-typed-runtime-ops
```

It writes `target/cranelift-only/runtime-helper-audit.json` and Markdown, then
checks required families, stable IDs, complete audit metadata, native-tier
callability, forbidden generic-dispatch shortcuts, the IR-to-helper mapping,
the shared VM semantics, and the complete workspace graph.

## Prompt 6 unified native calls

All IR call instructions now enter `RegionNativeCall`: function, instance and
static method, closure, callable, pipe, both by-reference call destinations,
and static/dynamic construction. The call contract retains named and unpacked
arguments, lvalue/by-reference metadata, strict-types mode, direct arity, and
an explicit by-reference return destination.

`JitNativeCallFrame` is the single ABI record for native PHP frames. It carries
function/region identity, local and temporary slot tables, caller continuation,
result slot, receiver and class context, exception and trace metadata, and
generator/fiber handles. `JitNativeCallArgument` carries values, name hashes,
unpack/by-reference flags, and caller lvalue slots. Runtime callback kinds for
builtins, magic methods, hooks, autoload, error handlers, shutdown functions,
and destructors use the same target tags.

Fixed-arity scalar calls within a generation remain compiled-to-compiled.
Dynamic or complex calls write the ABI records directly into native stack slots
and call `jit_native_call_dispatch_abi`. A missing or stale target returns
`COMPILE_REQUIRED`; the trampoline has no executor-loop entry. Live absolute
addresses are represented by `JitNativeIndirectionEntry` and checked against a
deployment generation instead of being part of persisted target identity. The
retired tailcall/resume statuses and their orphan emitter source are deleted.

Run the gate with:

```sh
nix develop -c just cranelift-native-calls
```

The generated `target/cranelift-only/native-call-model.{json,md}` records the
ABI layout and coverage. The gate rejects missing call forms, VM call objects
in native lowering, interpreter re-entry, and the old tailcall/resume protocol.

## Prompt 7 native control flow

Native PHP entries use one versioned status vocabulary with ABI-stable tags:
`Continue`, `Return`, `ReturnReference`, `Throw`, `Exit`,
`SuspendGenerator`, `SuspendFiber`, `RuntimeError`, `CompileRequired`, and
`RecompileRequested`. Helper-local success/fallback codes are not used as
native PHP frame outcomes. Direct calls and the dynamic trampoline propagate
the PHP status and value without Rust unwinding across generated code.

`EnterTry`, `LeaveTry`, `EndFinally`, `Throw`, and `MakeException` are explicit
`RegionNativeControl` operations. Return and exit terminators carry their
active finally target; generated code stores pending status/value state and
runs the compiled finally block before leaving the frame. Exception handler
blocks have tagged native resume entries. `JitRegionStateMetadata` selects
catch/finally targets, and `invoke_i64_with_native_unwind` resumes those native
blocks without constructing an interpreter frame or entering an exception
dispatch loop.

Every compiled handle publishes exception tables, safepoints, baseline live
slot roots, optimized-root requirements, and continuation-attributed native PC
ranges. Native PCs resolve back to exact IR byte spans for PHP backtraces.
The C-compatible frame/root/handler records contain no Rust collections.
Destructor ordering points—local overwrite, discard, frame return, exception
unwind, and request shutdown—are stable ABI tags and destructors use the
unified native call kind introduced in Prompt 6.

Run the gate with:

```sh
nix develop -c just cranelift-native-control
```

It writes `target/cranelift-only/native-control-flow.{json,md}`, validates the
ABI and source structure, executes native catch/throw/return/exit/finally
tests, exercises the existing PHP-visible exception/destructor/error-handler/
shutdown/GC ordering fixtures, and checks the complete workspace graph.

## Prompt 8 native generator and fiber state machines

`Yield`, `YieldFrom`, and `Fiber::suspend` lower to `RegionNativeSuspend`.
Each suspension continuation receives a stable artifact-local resume ID in the
`0x40000000` namespace and a generated Cranelift resume block. Initial entry,
send/resume, throw injection, return/getReturn, delegation completion, and
finally execution therefore continue in generated control flow. The resume
API performs one native entry invocation; it contains no Rust instruction loop
and has no dependency on the VM's Dense, Rich, or IR continuation types.

Suspension writes live locals and temporaries, yielded key/value, delegation
identity, exception input, root metadata, function/version identity, and the
continuation ID into ABI-stable state. `JitNativeGeneratorState` and
`JitNativeFiberState` retain the owning native generation. Invalidation either
keeps that generation live or calls the explicit safe-suspension transition
that installs a new native version and resume ID. Suspension metadata is part
of the immutable compiled handle reused by the process code cache.

Runtime code remains responsible for scheduling, generator delegation, heap
ownership, and misuse diagnostics. Generated code implements PHP control on
both sides of every suspension.

Run the gate with:

```sh
nix develop -c just cranelift-native-suspensions
```

It writes `target/cranelift-only/native-suspensions.{json,md}`, verifies state
layout, generation ownership, native resume coverage and forbidden dispatch
dependencies, executes focused generated yield/yield-from/send/throw/finally/
fiber tests, runs the existing generator and fiber semantic fixtures, and
checks all workspace targets.

## Prompt 9 native dynamic source compilation

`Include`, `Eval`, runtime function/class declaration, and known closure
creation lower to `RegionNativeDynamicCode`. Generated code calls the typed
`JitNativeDynamicCodeTrampoline`; the operation resolves and validates source,
compiles the complete unit and its declarations, atomically publishes native
entries, and only then invokes the requested entry. Missing compiler context
returns the explicit `COMPILE_REQUIRED` status and never selects a first-run
fallback executor.

`DynamicCodeCompileOnce` keys artifacts by exact source, dependencies, semantic
configuration, runtime ABI, and target CPU. One owner compiles outside all
coordinator locks while concurrent consumers wait for publication. This makes
nested compilation of a distinct key safe, detects recursive ownership of the
same key, caches compile errors deterministically, participates in process and
validated restart caches, and exposes explicit child-after-fork
reinitialization. Runtime declarations and known closures reference bodies
included in the same native call graph before their PHP-visible publication.
Autoload remains the typed native callback kind from Prompt 6.

Run the gate with:

```sh
nix develop -c just cranelift-native-dynamic-code
```

It writes `target/cranelift-only/native-dynamic-code.{json,md}`, verifies the
ABI, Region IR, generated callout, compile-once/cache/fork contracts and source
invariants, executes focused native/concurrency/cache/error tests plus existing
include/eval/autoload semantic fixtures, and checks every workspace target.

## Prompt 10 native slow paths and version transitions

Every compiled function now publishes a non-speculative baseline artifact and
an exact generated entry for each legal instruction continuation. Resume IDs in
the `0x20000000` namespace reconstruct live locals, live registers, pending
exception/finally control, result destination, function identity, and source
version from `JitNativeTransitionState`. Cranelift splits source blocks at
instruction boundaries, so entering a continuation never repeats preceding
output, mutation, calls, or other PHP-visible effects.

Optimized `RECOMPILE_REQUESTED` exits use
`invoke_i64_with_native_transition` to select a published baseline function
entry, including a nested callee, and enter the exact continuation. The same
metadata supports less-specialized versions. Loop promotion and guard exits
are native-to-native: baseline loop OSR enters generated optimized code, while
an optimized guard reconstructs state into a baseline generated entry. A
version transition returns only native control statuses; it has no alternate
executor target and never restarts a function after an observable effect.

Run the gate with:

```sh
nix develop -c just cranelift-native-transitions
```

It writes `target/cranelift-only/native-version-transitions.{json,md}`, audits
the state and exact-entry source contracts, proves nested callee routing and
no effect replay, exercises native OSR/overflow metadata, retains PHP-visible
overflow and cache-guard semantics, and checks all workspace targets.

## Prompt 11 native execution coordinator

The former test-only executor has been removed rather than hidden behind a
configuration flag. `php_vm` now contains only compiled-unit preparation,
request/cache metadata, native helper ABIs, mandatory Cranelift compilation,
native entry publication, and outer result assembly. The Dense bytecode tree,
Rich dispatch modules, match-based IR loop, deoptimization/resume protocol,
quickening, superinstructions, interpreter OSR, and their public options and
reports no longer exist.

`php_executor`, `php-vm`, and the server expose baseline or optimizing native
profiles only. Removed executor flags are rejected instead of being accepted
as no-ops. Runtime counters and request profiles describe native compilation,
entries, helpers, caches, and side exits; they do not retain migration-era
instruction-family totals. Differential validation uses the pinned external
PHP binary and no in-crate migration oracle.

Run the gate with:

```sh
nix develop -c just cranelift-native-executor
```

The gate verifies the deleted paths and public names, checks an empty stage-11
allowlist, builds every workspace target, builds the release server, and scans
its symbols for Cranelift while rejecting retired executor entry symbols.
