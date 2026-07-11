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

Completed ownership slices already remove instrumentation/JIT state, call
models, continuation/control models, foreach iteration, and request-local
extension adapter state from the facade. `ExecutionState` now contains one
`BuiltinAdapterState` subsystem instead of directly owning extension clients,
stream-wrapper state, and the typed `BuiltinRequestState`. HTTP response,
upload, session, and SAPI state similarly live behind one
`RequestLifecycleState` subsystem. The remaining execution state has a dedicated
`execution_state` owner rather than being declared in the facade. Request-local
declaration models, error/shutdown state, destructor ownership, and GC root
scanning have moved with it. Class constants, cloning, autoload, class/object
introspection, aliases, dependency validation, and construction now have a
bounded `class_operations` owner. Function/fiber target resolution, dense-plan
calls, array callables, and closure binding now complete the bounded `calls`
owner rather than routing through facade methods. Callable validation,
acquisition, presentation, and internal dispatch cacheability also live in
`calls` instead of remaining as facade free functions. Trivial method inlining,
route selection, dense route execution, bound methods, and invokable-object
dispatch now live in the separate `method_dispatch` owner. Argument binding,
reference acquisition, parameter/return/property type enforcement, and
sensitive-parameter presentation now live in `arguments`; closure creation,
capture materialization, and `$this` initialization live in
`closure_operations`. Global, lexical, predefined, and class-constant
resolution now lives in `class_operations`; state-aware class/method relation
queries and property resolution live in `class_relations` and
`property_resolution` respectively.

## Target Ownership

`rich_dispatch` now owns the complete rich execution cursor instead of leaving
it embedded in the facade. Its initial 15,212-line move is a migration boundary,
not an accepted final file size: direct foreach, exception-control, and
array/dimension handlers have since moved to focused owners. Opcode-family
extraction must ratchet the cursor below the repository's 5,000-line default
while `vm/mod.rs` remains below the 2,500-line facade limit. Neither large-file
baseline may increase during that migration.

| Owner | Owns | Must not own |
| --- | --- | --- |
| `vm/mod.rs` | public re-exports, `Vm` construction, request lifecycle, top-level execute/resume orchestration | opcode handlers, extension implementations, JIT ABI, filesystem compilation, SPL behavior |
| `execution_state` | frames, exception/control state, request-local declaration tables, deadline state, GC roots | builtin dispatch or server state |
| `rich_dispatch` | IR cursor and direct opcode selection | extension implementations or backend-specific JIT details |
| `rich_array_dispatch` | array literals and dimension read/write/probe/unset handlers | local-variable lifecycle or generic property rules |
| `dense_dispatch` | dense cursor and direct opcode selection | a second semantic implementation |
| `arguments` | argument binding, reference acquisition, type enforcement, and parameter diagnostics | call routing or opcode dispatch |
| `closure_operations` | closure values, captures, binding context, and `$this` initialization | generic function dispatch or argument binding |
| `calls` | activation, function/closure calls, return and unwind | generic property rules or SPL collections |
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
              |
       direct opcode-family handlers
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
