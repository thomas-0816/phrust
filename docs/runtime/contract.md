# Native Runtime Contract

This contract describes the post-cutover execution boundary. Historical
interpreter, Dense-bytecode, quickening, and superinstruction contracts were
retired by the Cranelift-only cutover.

## Input pipeline

There is exactly one PHP input pipeline:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir
-> executable Region IR -> Cranelift -> php_runtime helpers
```

`php_ir::IrUnit` is the authoritative semantic compiler input. Source byte
spans remain the source of truth; generated code and native continuations retain
source-map metadata.

## Execution contract

- Cranelift is mandatory and has no runtime off switch.
- Baseline and optimizing are native compiler tiers, not engines.
- Every known function body must have a baseline native compilation record
  before its unit is executable.
- Missing lowering or ABI support fails setup with a concrete diagnostic.
- Native calls use typed frame/result records and published indirection entries.
- Optimized exits target exact baseline-native continuations.
- Include, eval, and runtime declarations use the native dynamic-code compiler
  boundary and publish before first execution.
- Generator and fiber suspension persists native continuation identity and live
  state; resume does not dispatch PHP instructions in Rust.
- Eligible native entries may be loaded from a validated PNA2 artifact. The
  cache never changes language semantics and never supplies a second executor.

## Runtime ownership

`php_runtime` owns values, references, Copy-on-Write behavior, arrays, objects,
builtins, diagnostics, output, and typed runtime operations. `php_vm` owns outer
request coordination, native publication, helper wiring, caches, and result
assembly. `php_jit` owns Region IR, Cranelift lowering, code memory, native ABI
metadata, continuations, and version transitions.

Parser and semantic diagnostics stay in their frontend crates. Native compile
diagnostics and runtime diagnostics remain separate and stable.

## Unsupported behavior

Unsupported language or runtime behavior must return a deterministic diagnostic
or documented known gap. It must never produce a plausible substitute value,
select another executor, or silently continue after a missing native operation.

The core runtime scope still excludes a complete standard library, Zend
extension ABI, FPM/SAPI emulation, Opcache, quickening, inline caches outside
the native call/resolver design, and a JIT distinct from the mandatory
Cranelift pipeline.

## Validation

The architectural acceptance gate is:

```sh
nix develop -c just cranelift-native-executor
nix develop -c just cranelift-native-cache
```

Runtime behavior gates invoke the pinned `REFERENCE_PHP` 8.5.7 binary when it
is available. Generated local reports stay under `target/`.
