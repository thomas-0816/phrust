# ADR 0018: Cranelift Memory Safety Boundary

## Status

Accepted for Performance.

## Context

The Cranelift addendum now contains constrained native execution paths for a
small set of hot shapes. Those paths produce raw executable function pointers
through Cranelift's `JITModule` and enter them through a fixed `extern "C"` ABI.

Performance does not own a production code-cache reclamation scheme, Zend ABI,
or native stack/frame integration. The safety boundary must therefore be
explicit and conservative.

## Decision

- Keep `jit-cranelift` default-off in Cargo features.
- Require runtime native-execution opt-in before native entries are compiled or
  invoked.
- Delegate Cranelift-generated native-entry allocation and W^X transitions to
  Cranelift's JIT memory provider.
- Keep repository-emitted machine-code experiments behind
  `php_jit::code_memory::CodeMemory`; do not add ad hoc `mmap` or `mprotect`
  executable-code paths elsewhere.
- Leak finalized `JITModule` values for process lifetime so cloneable
  `JitFunctionHandle` values cannot outlive their executable code.
- Validate native calls with `JIT_RUNTIME_ABI_HASH` before raw-address
  transmute.
- Pass only integer values, opaque `usize` pointers, and out pointers across the
  native boundary. Do not pass Rust references into generated code.
- Keep VM value pointers synchronous and stack-owned. Generated code and helper
  shims must not retain those pointers after returning.
- Transfer allocated PHP values back to the VM with `Box::into_raw` only on
  success, and reconstruct the `Box<Value>` immediately in the VM call path.
- Treat unsupported or unproved fast paths as disabled and fall back to the
  interpreter or existing VM helper dispatch.

## Consequences

The accepted Performance tradeoff is memory growth for native entries compiled
during a process. That is safer than introducing premature reclamation for raw
function pointers. A future production JIT must replace the Cranelift-module
leak with handle invalidation, code-cache lifecycle ownership, and
platform-specific reclamation tests before broad native execution can be enabled
by default.

## Validation

The focused lifecycle test proves copied handles stay callable after the
original handle value is dropped:

```bash
nix develop -c cargo test -p php_jit --features jit-cranelift cranelift_native_handle_copy_survives_original_handle_drop
```

The full gate set is:

```bash
nix develop -c cargo test --workspace --features jit-cranelift
nix develop -c just verify-cranelift
```
