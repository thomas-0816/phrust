# Performance Safety Audit

## Work item: Cache, Adaptive Runtime, And JIT Audit

Work item reviewed the Performance bytecode cache, optimizer-adjacent
adaptive runtime state, quickening, inline caches, tiering, and experimental
JIT. The audited implementation surface contains no Rust `unsafe` blocks in:

- `crates/php_bytecode_cache`;
- `crates/php_jit`;
- `crates/php_vm/src/inline_cache.rs`;
- `crates/php_vm/src/quickening.rs`;
- `crates/php_vm/src/tiering.rs`.

The optional gate `just safety-audit-smoke` now enforces that scoped
`unsafe` scan, runs bytecode-cache negative tests, and attempts a small Miri
cache test only when the active toolchain supports `cargo miri`. Miri absence
is a clean skip, not a failure.

### Bytecode Cache

Audit result:

- Cache bytes are untrusted input. `CacheArtifact::from_bytes` validates magic,
  format version, target triple, PHP target, metadata length, metadata JSON,
  payload length, future versions, and fingerprint before returning an artifact.
- IR payload loading runs `php_ir::verify_unit` before execution. Payloads that
  deserialize but fail verifier checks are rejected as typed cache-load errors.
- Cache filenames are derived from hex fingerprint digests and suffixed with
  `.phbc`; source paths do not become cache filenames.
- `--clear-bytecode-cache` removes only `.phbc` files in the configured cache
  directory.
- Corrupt or stale cache data records a cache miss/load error and falls back to
  compile-from-source in the CLI path.

Coverage:

- `cargo test -p php_bytecode_cache corrupt` covers malformed cache bytes,
  corrupt metadata JSON, corrupt IR payloads, and typed load errors.
- `cargo test -p php_bytecode_cache bytecode_cache` covers fingerprint
  mismatch before payload decode and verifier rejection.
- `cargo test -p php_vm_cli bytecode_cache` covers corrupt cache fallback,
  source-change misses, opt-level misses, non-hex digest path-component
  rejection, and read/write hit behavior.

Residual constraints:

- A user-selected cache directory is trusted as a storage root for local CLI
  tests. The current file name construction prevents traversal through cache
  keys, but the CLI intentionally does not sandbox arbitrary user-provided cache
  directories.
- Include and Composer dependency graphs remain known gaps for cache-hit safety
  and are tracked in `docs/performance/known-gaps.md`.

### Quickening, Inline Caches, And Tiering

Audit result:

- Quickening and inline-cache state is request-local VM side-table state. It is
  cleared at the start of each `Vm::execute` request and does not mutate the IR
  artifact.
- Guard failures route through the shared fallback/dequickening protocol and
  call the baseline interpreter path exactly once.
- IC entries store stable ids, normalized names, epochs, and metadata handles;
  they do not store raw Rust references to functions, classes, object storage,
  properties, arrays, references, or VM frames.
- Function/class/include/autoload config changes use monotonic epochs. Stale
  epochs invalidate or refresh IC entries before reusing cached metadata.
- Work item tiering counters are saturating request-local integers. They do
  not create control-flow edges and cannot introduce tiering loops.
- `--tiering=off` disables adaptive quickening observations and JIT attempts,
  keeping execution on the interpreter even when other adaptive flags are set.

Coverage:

- Inline-cache unit tests cover function/method/property/class/static/include
  and autoload epoch invalidation.
- VM tests cover include file metadata invalidation, autoload invalidation after
  `spl_autoload_register`, negative autoload invalidation after include, and
  megamorphic/dequickening type-change fallback.
- Tiering unit tests cover threshold promotion, disabled tiering, backedge
  counting, and megamorphic guard-failure fallback to the interpreter.

### JIT And Executable Memory

Audit result:

- JIT remains default-off at both Cargo feature and CLI levels.
- Feature-off builds do not depend on Cranelift or executable-memory support.
- Feature-on Cranelift lowering verifies IR text before any native entry can be
  used.
- Guarded native entries exist only for narrow eligible shapes and require the
  explicit native-execution opt-in, ABI checks, helper boundaries, and
  interpreter fallback.
- Executable-memory ownership is split between Cranelift's JIT memory provider
  for Cranelift-generated entries and `php_jit::code_memory` for
  repository-emitted machine-code experiments. No ad hoc executable-memory path
  is accepted.
- Production/default-on native execution, persistent native-code caches, and
  broad VM native-frame integration remain out of scope.

Coverage:

- `jit-smoke` runs feature-off and feature-on JIT tests, VM JIT A/B output
  comparison, rejected-function fallback, and counter assertions.
- Feature-on VM tests cover JIT-off no-compile behavior, eligible int-leaf
  execution, and rejected fallback.

### Panic And Malformed Input Boundary

Audit result:

- Public cache APIs return typed errors for malformed input rather than
  panicking.
- Public VM and CLI paths continue to use `ExecutionStatus` and structured
  diagnostics for unsupported runtime features.
- Internal `expect` calls remain in invariants where the VM has just pushed a
  frame or tests have intentionally constructed fixture state; they are not
  exposed as public malformed-input parsing APIs.

No Performance safety issue currently requires disabling bytecode cache,
quickening, inline caches, tiering, or default-off JIT. Cranelift native
execution is still Cargo-feature-gated and runtime default-off; its executable
memory and ABI boundary is audited separately in
`docs/performance/cranelift/safety-audit.md`.

### Developer Commands

Run the safety-focused gate:

```bash
nix develop -c just safety-audit-smoke
```

Run adjacent gates when investigating a safety-sensitive change:

```bash
nix develop -c just cache-roundtrip
nix develop -c just quickening-smoke
nix develop -c just inline-cache-smoke
nix develop -c just jit-smoke
nix develop -c just verify-performance
```

Manual unsafe scan used by the smoke:

```bash
nix develop -c rg -n '\bunsafe\b' \
  crates/php_bytecode_cache \
  crates/php_vm/src/inline_cache.rs \
  crates/php_vm/src/quickening.rs \
  crates/php_vm/src/tiering.rs

nix develop -c rg -n '\bunsafe\b' crates/php_jit/src \
  --glob '!lib.rs' \
  --glob '!helpers.rs' \
  --glob '!cranelift_lowering.rs'
```

The excluded `php_jit` files contain the feature-gated Cranelift native-call
boundary and must stay covered by `docs/performance/cranelift/safety-audit.md`,
`docs/adr/0018-cranelift-memory-safety.md`, and
`just verify-cranelift`.

### Troubleshooting

- New default-surface `unsafe`: document the boundary here, isolate it behind a
  small wrapper, add negative tests, and keep feature-off/default-off behavior
  unchanged.
- New Cranelift native-boundary `unsafe`: document it in
  `docs/performance/cranelift/safety-audit.md`, keep `jit-cranelift` default-off,
  and validate it with `just verify-cranelift`.
- Miri unavailable: the safety smoke skips cleanly when `cargo miri` is absent
  or unusable for the active toolchain. That skip is acceptable; do not hide
  normal Rust test failures behind it.
- Cache corruption: reproduce with `cargo test -p php_bytecode_cache corrupt`
  and ensure CLI execution falls back rather than aborting.
- Stale handles or epochs: prefer stable ids, names, epochs, and metadata
  handles; do not store raw references into VM frames, arrays, objects, or
  class/function tables.

## Work item: VM Frame Reuse

The VM now keeps a request-local frame pool inside `CallStack` for normal
function activations. Completed normal calls recycle their frame after the VM
has exported shared locals and captured any by-reference return cell.
Each `Frame` carries an explicit reuse-eligibility flag, so shared
`pop_recycle()` exits cannot accidentally pool an activation that was classified
as unsafe at call entry.

Safety constraints:

- Pooled frames are cleared before reuse. Registers, locals, arguments, and
  class context fields are dropped so the pool does not keep PHP-visible values,
  references, objects, or destructor roots alive.
- Fresh non-pooled frames are used for closure captures, by-reference params or
  returns, generator/fiber continuations, class contexts, shared top-level
  locals, try/finally bodies, and object-allocation bodies that may retain
  destructor-sensitive values.
- Generator and fiber suspension paths still use ownership-moving `pop()` and
  store the frame in their continuation. They do not recycle suspended frames.
- Exception and fiber propagation paths keep ownership semantics unchanged.
  The conservative pool is used for successful normal returns and setup
  failures before dispatch, not for uncertain unwind boundaries.
- The implementation uses safe Rust only and does not extend lifetimes.

Allocation counters:

- `frame_allocations` counts fresh frame allocations for function activations.
- `frame_reuses` counts activations served from the request-local frame pool.
- `frames_allocated` and `frames_reused` are compatibility aliases for the same
  activation counts.
- `register_files_allocated` and `register_files_reused` track the register-file
  allocation/reuse event coupled to each frame activation.
- `frame_reuse_blocked_by_reason` records conservative fallback reasons.

Coverage:

- call-heavy loop shows frame reuse counters,
- recursive calls preserve nested active frames,
- closure captures and by-reference params execute with explicit non-reuse
  counters,
- supported finally-through-call control flow remains observable,
- destructor output remains observable while object-allocation frames stay
  non-pooled,
- generator and fiber suspension smoke tests preserve existing output.

## Work item: JIT ABI Boundary

`crates/php_jit/src/abi.rs` defines the VM/JIT boundary for future native
experiments. The boundary is intentionally handle-based and by-value.

Safety constraints:

- VM context and frame identity cross the ABI as non-zero opaque handles, not
  raw pointers or Rust references.
- Frame/register metadata is exposed as counts plus IR IDs. Future JIT code can
  ask whether a register/local index is in bounds, but cannot borrow the VM
  frame, register file, local file, `Value`, `Slot`, reference cell, array,
  object, resource, generator, or fiber internals.
- Heap-backed PHP values cross the boundary as `JitAbiValue::Opaque` with a
  value-kind tag and VM-owned handle. Primitive int/bool/null/float values are
  copied by value.
- Bailout/deopt, runtime-callout, region-result, and exception propagation use
  owned enums and strings. They do not extend lifetimes across the boundary.
- The implementation uses safe Rust only. Work item adds no `unsafe`, no
  executable memory, and no native execution path.

Coverage:

- zero handles are rejected for context, frame, and opaque value handles,
- frame views perform register/local bounds checks from exported counts,
- ABI value tests cover primitive and opaque heap-value boundaries,
- bailout/deopt and exception marker tests cover owned propagation metadata,
- `jit-smoke` runs the default and `jit-cranelift` feature test sets while
  still recording native execution as skipped.

## Work item: Cranelift IR Lowering Prototype

`crates/php_jit/src/cranelift_lowering.rs` is compiled only when the
`jit-cranelift` feature is enabled. It began as a CLIF-only lowering prototype
and now owns the constrained native-entry experiment audited in
`docs/performance/cranelift/safety-audit.md`.

Safety constraints:

- The module emits Cranelift IR text only. It does not allocate executable
  memory, does not expose a callable function pointer, and does not install a
  VM execution path.
- Lowering starts with the conservative Work item eligibility analyzer, then
  narrows the accepted subset further to integer constants, integer
  add/sub/mul, register moves, and an integer return.
- Unsupported instructions, operands, constants, control flow, missing
  registers, and by-reference returns produce typed `JIT_CRANELIFT_REJECT_*`
  errors rather than being silently compiled.
- The implementation uses safe Rust only and preserves feature-off builds
  without Cranelift dependencies.

Coverage:

- feature-on tests assert generated Cranelift IR contains integer constants,
  `iadd`, `isub`, `imul`, and `return`,
- unsupported division is rejected before lowering through typed eligibility
  rejection,
- eligibility-accepted boolean constants are rejected by the stricter 07.52
  integer-only lowerer,
- `jit-smoke` runs both feature-off and `jit-cranelift` feature test sets while
  still recording native execution as skipped.

## Work item: Guarded Int-Leaf JIT Execution

The VM now has an experimental JIT execution tier behind both Cargo feature
`jit-cranelift` and CLI/runtime `--jit=on`. The tier is request-local and
default off.

Safety constraints:

- The interpreter remains the source of truth. Functions run on the interpreter
  until they pass a warmup threshold, and any compile rejection or guard failure
  falls back to interpreter execution.
- Compilation first calls the Work item eligibility analyzer through the
  Work item Cranelift lowerer. Only successfully verified tiny integer leaf
  functions can enter the JIT execution path.
- Native entries use a fixed `extern "C"` ABI, ABI-hash checks, opaque pointer
  arguments, and status/side-exit returns. They do not resume at arbitrary
  native PCs or expose partial VM frames.
- The accepted runtime subset is integer constants, local loads/stores, moves,
  checked add/sub/mul, and integer return. Checked arithmetic overflow is a
  bailout, not wrapping behavior.
- Functions with methods, closures, generators, fibers, captures, `$this`,
  shared top-level locals, by-reference parameters, typed parameters, declared
  return types, defaults, variadics, arrays, objects, calls, exceptions, or
  non-integer values fall back.

Coverage:

- feature-on VM tests cover hot int-add leaf execution after warmup,
  rejected-function fallback, and `jit=off`/`jit=on` output identity,
- CLI parser tests cover `--jit=off|on` and invalid modes,
- `jit-smoke` builds the feature-on CLI, compares `--jit=off` and `--jit=on`
  output for the hot int-leaf fixture, checks a rejected function fallback
  fixture, and asserts `jit_compile_attempts`, `jit_compiled`, `jit_executed`,
  and `jit_bailouts`,
- the smoke artifact records `native_machine_code_execution: false`.
