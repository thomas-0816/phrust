# Cranelift Constrained Native Backend

A later stage added `CraneliftNoExecBackend`, a feature-gated backend adapter
behind `jit-cranelift`. a later stage keeps the type name for compatibility
but extends the backend with the first executable subset: constant integer
return leaf functions. a later stage adds helper-call integer add/mul native
execution while preserving interpreter fallback as the correctness boundary.
A later stage replaces that executable arithmetic path for eligible
add/sub/mul with inline checked Cranelift integer operations.
A later stage extends the inline path to simple branches and counted loops
over eligible integer operations.

## Behavior

The backend implements `JitBackendApi` and can:

- identify itself as `JitBackend::CraneliftExperiment`;
- reject native entry when runtime permission is disabled;
- require explicit IR unit and function context before lowering;
- lower eligible IR into verified Cranelift IR text through the existing
  lowering path;
- return no executable handle for non-executable subsets;
- compile the constant-return native subset to a JIT entry only when
  `allow_native_execution` is true;
- compile simple int add/sub/mul functions to inline checked integer operations
  when `allow_native_execution` is true;
- compile simple multi-block int control flow with comparisons, jumps, returns,
  and counted loops when the loop body contains only eligible int operations;
- attach the runtime ABI hash and native code byte count to the JIT handle;
- report deterministic diagnostics for smoke gates.

In this subset, native execution is restricted to ordinary leaf functions
with plain `int` parameters, an explicit `int` return type, and a body made only
of integer constants, moves, no-ops, and a return. All other shapes may still be
lowered to verified CLIF for diagnostics, but they return
`JitCompileStatus::Rejected` and execute through the interpreter.

In this subset, native helper-call execution is restricted to ordinary
leaf functions with plain `int` parameters, an explicit `int` return type, and a
single block containing integer constants, local loads, moves, add, mul, and
return. Add and mul are never lowered as inline raw integer operations. They
call `phrust_jit_i64_add_checked` or `phrust_jit_i64_mul_checked`, check the
returned status immediately, and return non-zero status to the VM when the
interpreter must resume.

In this subset, the same eligible single-block arithmetic subset lowers
add, sub, and mul to Cranelift `sadd_overflow`, `ssub_overflow`, and
`smul_overflow`. Non-overflowing operations continue in native code and report
`fast_path_hits`. Overflow returns a stable overflow status through the native
ABI so the VM records an `overflow` side exit, `overflow_exits`, and
`slow_path_calls` before interpreter fallback. The inline path does not perform
float coercion or weak numeric-string conversion.

In this subset, the native subset accepts simple multi-block CFGs made of
conditional branches over int comparisons, unconditional jumps, returns, and
counted loops whose loop variable, bound, and body are all in the eligible int
subset. The lowering uses Cranelift blocks for PHP IR blocks, Cranelift
variables for loop-carried locals, signed integer comparison condition codes,
and checked add/sub/mul overflow branches. Calls, array mutation, complex
`break`/`continue` with exception/finally behavior, and OSR remain outside the
native loop subset and fall back deterministically.

The VM invokes native code only through `JitFunctionHandle::invoke_i64`, which
checks `JIT_RUNTIME_ABI_HASH` before dispatch. Arity mismatches, ABI mismatches,
compile failures, unsupported IR, and native invoke failures all fall back to
the interpreter.

## Validation

```bash
nix develop -c cargo test -p php_jit --features jit-cranelift cranelift_no_exec
nix develop -c cargo test -p php_jit --features jit-cranelift cranelift_backend
nix develop -c cargo test -p php_jit --features jit-cranelift helper
nix develop -c cargo test -p php_vm --features jit-cranelift cranelift_
nix develop -c just jit-cranelift-smoke
nix develop -c just jit-cranelift-bench-smoke
```
