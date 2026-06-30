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
- execution option enums, counters, and result metadata exposed through
  `VmOptions` and `VmResult`;
- `CompiledUnit` and include-loader types needed by the executor and cache
  integration.

Use `php_vm::experimental` for performance and VM-internal instrumentation:

- dense bytecode views, counters, quickening, inline caches, tiering, fallback
  policy, persistent feedback, and deopt metadata;
- frame/register internals and alias-state helpers.

The root re-exports remain for compatibility with older local code. Prompted
refactors should migrate public boundary crates to the facades before reducing
root exports.

VM implementation ownership is split by behavior, not by prompt history:

| Behavior | Owner |
| --- | --- |
| Stable compile/execute API | `php_vm::api` |
| Optimization and instrumentation experiments | `php_vm::experimental` |
| VM options and result construction | `crates/php_vm/src/vm/options.rs`, `crates/php_vm/src/vm/result.rs` |
| Dispatch loop, control flow, exceptions, and finally unwinding | `crates/php_vm/src/vm/mod.rs` |
| Calls and argument binding | `crates/php_vm/src/vm/arguments.rs` |
| Object and method fast dispatch | `crates/php_vm/src/vm/dense_method_dispatch.rs` |
| Include, require, eval, and autoload cache boundaries | `crates/php_vm/src/include.rs` plus the VM include/eval call sites |
| Generators and fibers | `crates/php_vm/src/vm/generator_fiber.rs` |
| Tracing, counters, tiering, quickening, and inline caches | `crates/php_vm/src/counters.rs`, `tiering.rs`, `quickening.rs`, and `inline_cache.rs` |
| Dense bytecode lowering and fallback metadata | `crates/php_vm/src/bytecode/mod.rs` |

New public execution features should first decide whether they belong in the
stable `api` facade or the `experimental` facade. VM implementation changes
should stay inside the owning module above, and broad root imports should not be
introduced in downstream crates when a facade path exists.
