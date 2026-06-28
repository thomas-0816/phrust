# API Facades

`php_runtime` and `php_vm` expose explicit facades so downstream crates do not
accidentally depend on implementation internals.

## Runtime

Use `php_runtime::api` for stable execution-facing types:

- values, arrays, strings, references, objects, resources, output, status, and
  runtime context;
- runtime and compile diagnostic payloads;
- builtin registry and compatibility types used by `php_std` and the VM.

Use `php_runtime::experimental` only for local instrumentation or experiments:

- debug GC snapshots and tracked-heap helpers;
- JIT array ABI helpers;
- numeric-string and layout-stat measurement modules.

Use `php_runtime::debug` when tests or VM diagnostics need weak handles,
`GcRoot`, `GcSnapshot`, `scan_roots()`, or `GcTrackedHeap`. These APIs inspect
runtime graph shape; they are not PHP-visible `gc_*` semantics.

The crate root still re-exports the historical broad surface as a compatibility
alias. New imports should not use the root when a facade path exists.

## VM

Use `php_vm::api` for stable execution-facing types:

- `Vm`, `VmOptions`, `VmResult`;
- execution option enums used by `VmOptions`;
- `CompiledUnit` and include-loader types needed by the executor and cache
  integration.

Use `php_vm::experimental` for performance and VM-internal instrumentation:

- dense bytecode views, counters, quickening, inline caches, tiering, fallback
  policy, persistent feedback, and deopt metadata;
- frame/register internals and alias-state helpers.

The root re-exports remain for compatibility with older local code. Prompted
refactors should migrate public boundary crates to the facades before reducing
root exports.
