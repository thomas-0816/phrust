# Native compile-record cache

The server shares one `VmWorkerState` across requests. Its compile-record cache
sits above Region IR construction and Cranelift emission, so a warm request can
reuse already-published native handles without rebuilding the same function
graph.

Every cache key names exactly one PHP function, tier, external-signature set,
and immutable unit identity. A successful record set must contain exactly that
function. Foreign function records are rejected before publication and are
never installed as aliases. Concurrent misses for one key use single-flight;
process-wide compiler parallelism and its bounded queue are configured with
`PHRUST_NATIVE_COMPILE_PARALLELISM` and
`PHRUST_NATIVE_COMPILE_QUEUE_LIMIT`.

Validate the policy with:

```bash
nix develop -c just function-on-demand-gate
```

## Restart-persistent PNA2 function artifacts

When `--native-cache` permits reads or writes, every executable IR unit is a
cache candidate, including entries of declaration-heavy include/eval units. The same
Cranelift lowering that publishes the process-local entry retains actual code
bytes and symbolic relocations before `JITModule` finalization. The cache
identity includes the requested function, and PNA2 stores only that function's
current native artifact:

- the requested PHP function entry and its future internal fragments;
- code bytes and internal-symbol relocations;
- stable helper IDs and names, never helper process addresses;
- deterministic `PRM4` state metadata for exceptions, native continuations, OSR,
  generators, and fibers.

Dormant functions and methods contribute no code, relocations, metadata, or
cache bytes. Unit-level data is declaration metadata; it does not imply native
body compilation.

Writers emit only PNA2. The loader accepts PNA1 for one migration window using
the same strict checksum, section, identity, relocation, and W^X validation;
the next successful write replaces it with PNA2. Function graphs shared by
multiple published entries are emitted once, while helper imports and internal
relocations are deduplicated at bundle scope. PNA2 also stores the uniform
packed-call ABI once for the function-entry section rather than repeating it in
every function record. PNA1/PRM3 remains read-only compatibility data during
the migration window.

Diagnostic linkage and footprint collection is deliberately separate from
clean timings. Use `just native-linkage-report COUNTERS`,
`just native-footprint-report CACHE_DIR`, and then
`just native-linkage-tranche-report` to assemble the complete C13 report tree.
Pass a native-smoke linkage JSON with `--smoke-linkage`; the builder records it
as synthetic, non-acceptance diagnostic evidence rather than treating its
direct-call ratio as a WordPress result.
The tranche builder records unavailable WordPress/RSS inputs as unmeasured with
an exact reason; structural gates never substitute for performance results.

Baseline same-unit userland calls use the typed native dispatcher and the
callee's generation-checked indirection cell. An unpublished cell enters the
single-flight compiler, publishes only after complete validation, and retries
the native call. Parameter strictness, coercion, references, and catchable
`TypeError` behavior therefore remain on the existing typed call contract.
Direct or guarded linking may be added only by a later optimizing recompile;
it must never widen baseline compilation.

Published functions from include/eval units execute through a scoped active-unit
view on the existing request context. The view swaps immutable unit metadata,
native entries, continuations, and callsite tables while retaining the same
value store, frame arena, output, globals, extension state, and request-owned
resources. Constant handles are materialized at the boundary, and return values
are materialized before restoring the caller unit. This removes the former
nested `NativeExecutionContext` and state-move/merge path for successful
cross-unit calls.

Function-on-demand baseline compilation does not form same-unit tail-call or
inlining groups. Fragment-local tail calls belong to the bounded-fragment
compiler and must not pull another PHP function into the artifact.

On AMD64, helper calls target an artifact-local `movabs`/`jmp` trampoline. The
loader resolves the trampoline immediate through the current versioned helper
registry after validating the artifact, then changes the mapping from writable
to executable. This keeps the original Cranelift `call rel32` in range without
persisting an absolute address.

Transition metadata follows the same 64-register publication bound as the
generated resume loaders. Do not publish entries after that bound: besides
advertising a nonexistent transition, cloning an ever-growing register prefix
turns metadata construction and serialization quadratic for generated
declaration units.

Focused restart checks:

```bash
nix develop -c cargo test -p php_vm vm_reloads_ --lib
nix develop -c cargo test -p php_jit native_helpers_publish_symbolic_restart_cache_relocations --lib
```

The default security/resource bounds remain 64 MiB per artifact, 32 MiB of
code per artifact, 65,536 relocations, and 512 MiB for the cache directory.
The unchanged WordPress 6.8.3 frontpage cache fitted inside those defaults
after bounding transition metadata; increasing the limits is not the remedy
for metadata growth.

For a real application, collect an instrumented request after at least one
warmup. The warm request must report zero `compile_attempts` before its latency
is treated as compilation-free performance evidence.
