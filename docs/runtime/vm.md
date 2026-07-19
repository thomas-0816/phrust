# Native PHP Execution Coordinator

See [vm-structure.md](vm-structure.md) for the current file ownership map.

`php_vm` consumes `php_ir::IrUnit` values produced by the existing frontend.
It does not parse source, lower a second IR, or execute instructions itself.
Every known function body is converted to executable Region IR and compiled by
Cranelift before the unit can run.

## Execution entry

`Vm::execute` optionally verifies the authoritative IR, compiles the complete
unit with the configured native tier and helper table, requires a published
entry for `IrUnit.entry`, and invokes that entry. Missing lowering, invalid ABI
metadata, or absent publication produces a deterministic native compile/setup
error. There is no managed fallback.

`Vm::prewarm_cranelift` performs the same bounded compilation and publication
work without entering application code.

Server-owned workers additionally support threshold publication. They run a
correct level-1 optimizing entry until its function-entry threshold is met,
then compile level 2 on a lower-priority background job. Both versions share
one atomic indirection cell; no request observes a partially published target,
and no interpreter or function restart participates in the transition.

## Native frames and control

Generated functions use the versioned native frame, call-result, continuation,
and status records from `php_jit`. Calls, returns, references, exceptions,
exit, generator/fiber suspension, dynamic source compilation, guard exits, and
OSR cross only those typed ABI boundaries. Baseline continuations are the
recovery target for optimized versions.

Runtime helpers own PHP value and operation semantics. They may request target
compilation or report a runtime status, but they cannot invoke another PHP
instruction executor.

## Result and diagnostics

The coordinator assembles stdout, return value, exit status, diagnostics,
native counters, and deterministic traces into the outer `VmResult`. Parser,
semantic, native compile, and runtime diagnostics remain separate.

## Validation

Run:

```sh
nix develop -c just cranelift-native-executor
```

Reference-dependent behavior is compared only with the pinned external PHP
8.5.7 binary through the runtime differential fixtures.
