# VM Decomposition Ownership Map

`crates/php_vm/src/vm/mod.rs` is a migration facade, not an acceptable final
owner for VM implementation. The checked architecture limit is a regression
ratchet while this map is implemented; it is not a target module size.

## Current Hotspots

The Prompt 11 starting point has 428 implementation methods in the parent
module. The largest cohesive but still unsplit regions are:

| Source region | Approximate size | Required owner |
| --- | ---: | --- |
| rich IR dispatch (`execute_function_inner`) | 15,200 lines | `rich_dispatch` plus focused opcode handlers |
| dense bytecode dispatch (`execute_bytecode_function`) | 6,100 lines | `dense_dispatch` plus shared operation helpers |
| builtin callback and sorting adapters | 6,000 lines | `builtin_adapter` and callback modules |
| calls, methods, closures, and object invocation | 5,500 lines | `calls` and `method_dispatch` |
| SPL iterator/container adapters | 3,300 lines | feature-specific internal-class adapters |
| autoload, reflection, and class dependency handling | 3,000 lines | `class_operations` and `reflection` |
| free class/object/serialization helpers | remaining tail | their owning object or internal-class module |

Line numbers are deliberately not part of this contract because each completed
slice changes them. `scripts/verify/architecture_inventory.py` is the
authoritative size inventory.

## Target Ownership

| Owner | Owns | Must not own |
| --- | --- | --- |
| `vm/mod.rs` | public re-exports, `Vm` construction, request lifecycle, top-level execute/resume orchestration | opcode handlers, extension implementations, JIT ABI, filesystem compilation, SPL behavior |
| `execution_state` | frames, exception/control state, request-local declaration tables, deadline state, GC roots | builtin dispatch or server state |
| `rich_dispatch` | IR cursor and direct opcode selection | extension implementations or backend-specific JIT details |
| `dense_dispatch` | dense cursor and direct opcode selection | a second semantic implementation |
| `calls` | argument binding, activation, function/closure calls, return and unwind | generic property rules or SPL collections |
| `method_dispatch` | method route selection, receiver/class context, method cache integration | class construction or extension methods |
| `class_operations` | class lookup, constants, static properties, construction, visibility, autoload metadata | reflection presentation or SPL implementation |
| `property_execution` | generic property read/write/isset/empty/hook rules | internal-class special cases |
| `builtin_adapter` | registry lookup, narrow call services, `BuiltinResult` translation | extension function implementation |
| feature adapters (`spl`, `reflection`, and peers) | only their internal-class bridge and migration shims | generic calls, dispatch, or object layout rules |
| `instrumentation` | counters, trace/profile attribution and disabled fast paths | execution semantics |
| `diagnostics` | VM diagnostic construction and source/frame translation | builtin or filesystem behavior |
| `jit_abi` | native ABI entry points, status values, handle conversion and side exits | interpreter dispatch policy |

Operation objects such as `CallRequest`, `PropertyAccessRequest`, or an
execution cursor are permitted only when every field belongs to that operation.
No module may introduce a general VM context or service locator.

## Dependency DAG

Dependencies point downward. Feature adapters may call the shared operation
layers, but shared layers never call feature implementations directly.

```text
public VM facade / request lifecycle
                |
        rich_dispatch   dense_dispatch
             \             /
              shared operations
        calls  method_dispatch  class_operations  property_execution
             \       |          /
                execution_state
              /        |        \
       diagnostics  instrumentation  runtime values

builtin_adapter ------> calls + execution_state + diagnostics
feature adapters -----> builtin_adapter/shared operations
jit_abi --------------> runtime values + JIT metadata
dispatch -------------> jit_abi through explicit entry points only
```

Forbidden edges are enforced incrementally by source-integrity and dependency
checks as each slice lands. In particular, dispatch must not import SPL,
reflection, filesystem compilation, or extension implementations.

## Migration Order

1. Move state models, JIT state, call request models, GC/destructor ownership,
   and stream-wrapper request state out of the facade.
2. Move complete call, method, class/property, builtin, and internal-class
   vertical slices; update all callers immediately.
3. Isolate dense and rich cursors, then extract opcode families without trait
   object dispatch in the hot loop.
4. Remove compatibility re-exports, the broad private prelude, and module-wide
   lint suppressions after their final users move.
5. Ratchet `vm/mod.rs` below 2,500 production lines and add a permanent owner
   check so implementation cannot return to the facade.

Each slice must preserve the JIT feature matrix and run its focused tests before
the aggregate VM/runtime gates. Parallel duplicate implementations are not a
migration mechanism.
