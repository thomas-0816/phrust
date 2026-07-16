# Native VM Structure

`php_vm` is the outer native PHP execution coordinator. It contains no opcode
loop and exposes no backend selector.

- `crates/php_vm/src/vm/mod.rs` verifies authoritative IR, compiles every
  function through Cranelift, publishes native entries, enters the unit entry,
  and assembles the outer execution result.
- `crates/php_vm/src/vm/options.rs` owns native compilation, runtime-context,
  tracing, include, and inline-cache configuration.
- `crates/php_vm/src/vm/jit_abi.rs` owns typed runtime-helper and native-call
  boundaries. Missing publication returns a native status; it never re-enters
  another executor.
- `crates/php_vm/src/vm/result.rs` owns the outer request result.
- `compiled_unit`, `dependency_units`, `include`, and `inline_cache` retain
  backend-neutral IR, dependency, source-map, resolver, and cache metadata.

Runtime semantics are implemented by typed operations in `php_runtime` and are
called from generated code through stable helper IDs and ABI records. Native
code generation lives in `php_jit`; frontend ownership remains
`php_lexer -> php_syntax -> php_ast -> php_semantics -> php_ir`.

The architectural gate is:

```sh
nix develop -c just cranelift-native-executor
```

It rejects retired executor paths and public engine types, runs native-focused
tests, checks every workspace target, builds the release server, and scans its
symbols for legacy entry points.
