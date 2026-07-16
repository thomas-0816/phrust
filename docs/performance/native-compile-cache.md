# Native compile-record cache

The server shares one `VmWorkerState` across requests. Its compile-record cache
sits above Region IR construction and Cranelift emission, so a warm request can
reuse already-published native handles without rebuilding the same function
graph.

The cache has two independently bounded LRU segments:

- primary entries are keys that execution explicitly requested;
- aliases are the additional function entries published by the same compiled
  graph.

Keeping the segments separate prevents a large PHP application from filling
the cache with graph aliases and evicting every include-entry key needed by the
next request. An alias hit is promoted to the primary segment. Both segments
remain bounded by the configured entry capacity.

Validate the policy with:

```bash
nix develop -c cargo test -p php_vm native_compile_cache --lib
```

## Restart-persistent PNA1 artifacts

When `--native-cache` permits reads or writes, every executable IR unit is a
cache candidate, including declaration-heavy include/eval units. The same
Cranelift lowering that publishes the process-local entry retains actual code
bytes and symbolic relocations before `JITModule` finalization. PNA1 stores:

- all native function entries in the unit;
- code bytes and internal-symbol relocations;
- stable helper IDs and names, never helper process addresses;
- deterministic `PRM3` state metadata for exceptions, native continuations, OSR,
  generators, and fibers.

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
