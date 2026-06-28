# Runtime VM

See `docs/runtime-vm-structure.md` for the current `php_vm::vm` file ownership map.

`php_vm` executes Runtime `php_ir::IrUnit` values produced from the existing
Semantic frontend frontend. It does not parse PHP source directly, does not run eval or
autoload callbacks, and does not emulate Zend ABI, extensions, opcache, or JIT.

## Execution Entry

The VM starts at `IrUnit.entry`, validates the unit when `VmOptions::verify_ir`
is enabled, creates a top-level frame, and dispatches basic blocks until a
terminator returns, throws an uncaught runtime error, or the `max_steps` guard
fires. Runtime stdout is collected by `php_runtime::OutputBuffer`; diagnostics
are structured `RuntimeDiagnostic` values with stable IDs.

The CLI entrypoint is:

```bash
nix develop -c cargo run -p php_vm_cli -- run fixtures/runtime/valid/hello.php
```

Fixture proof: `fixtures/runtime/valid/hello.php`,
`fixtures/runtime/valid/scalars/echo.php`, and `nix develop -c just
runtime-fixtures`.

## Frame Model

A frame owns:

- a `FunctionId` and current `BlockId`;
- a `LocalFile` for local slots;
- a `RegisterFile` for temporary registers;
- parameter, capture, and optional `$this` bindings;
- return and pending-control state used by calls, returns, and finally blocks.

Locals are stored as `ValueSlot`s so a local can either contain a concrete
`Value` or alias a `ReferenceCell`. Simple `$b =& $a` binds two local slots to
the same cell. Wider PHP reference and copy-on-write behavior is intentionally
classified as known gaps.

Fixture proof: `fixtures/runtime/valid/functions/local-scope.php`,
`fixtures/runtime/valid/functions/factorial.php`,
`fixtures/runtime/valid/references/local-alias.php`, and
`fixtures/runtime/valid/references/by-ref-param.php`.

## Call Stack

Direct user functions, closures, public methods, and selected builtin calls all
go through one call path:

- arguments are already evaluated into operands by IR lowering;
- required, optional, default, and variadic parameter metadata comes from
  `IrParam`;
- captures are copied into closure locals before parameter binding;
- instance method frames receive a `$this` object slot;
- simple public static methods execute without `$this`.

Selected builtins are resolved after user functions through
`php_runtime::BuiltinRegistry`. Dynamic function calls, array callables,
invokable objects, full callable fallback, and by-reference call semantics are
known gaps.

Fixture proof: `fixtures/runtime/valid/functions/simple.php`,
`fixtures/runtime/valid/functions/defaults.php`,
`fixtures/runtime/valid/functions/variadic-sum.php`,
`fixtures/runtime/valid/functions/closure-use.php`,
`fixtures/runtime/valid/objects/method-call.php`, and
`fixtures/runtime/known_gaps/functions/dynamic-call.php`.

## Dispatch and Control Flow

Instruction dispatch is register based. Ordinary instructions advance within a
block; terminators select the next block or return to the caller:

- `Jump`, `JumpIfFalse`, `JumpIfTrue`, and `JumpIf` implement branch edges;
- `Return` leaves the current frame;
- `RuntimeError` creates deterministic fatal diagnostics;
- the step counter prevents infinite interpreter loops in tests and CLI runs.

Loop lowering uses block-level jumps for `while`, `do`, `for`, `break`, and
`continue`. `switch`, `match`, short-circuit boolean operators, null-coalescing,
and ternary expressions are all represented as explicit blocks instead of VM
special cases.

Fixture proof: `fixtures/runtime/valid/control_flow/while-counter.php`,
`fixtures/runtime/valid/control_flow/for-loop.php`,
`fixtures/runtime/valid/control_flow/switch-fallthrough.php`,
`fixtures/runtime/valid/control_flow/match-success.php`, and
`fixtures/runtime/invalid/match-no-arm.php`.

## Exceptions

The exception MVP uses VM handlers, not PHP's full Throwable hierarchy:

- `EnterTry` pushes a handler with optional catch and finally targets;
- `LeaveTry` removes the handler on normal flow;
- `Throw` transfers control through the handler stack;
- `EndFinally` resumes pending return or throw flow;
- `MakeException` builds the VM-internal `Exception` object with a public
  `message` property.

MVP catches support the implemented `Exception`/`Throwable` path. Other catch
types, full Error/Throwable hierarchy, stacktrace formatting, nested edge
matrices, and catch-thrown finally behavior remain documented known gaps.

Fixture proof: `fixtures/runtime/valid/exceptions/catch-exception.php`,
`fixtures/runtime/valid/exceptions/catch-finally.php`,
`fixtures/runtime/valid/exceptions/finally-return.php`,
`fixtures/runtime/invalid/exceptions/throw-uncaught.php`, and
`fixtures/runtime/invalid/exceptions/nonmatching-catch-type.php`.

## Include Execution

`Include` evaluates its path operand, resolves it relative to the currently
executing file, and accepts only canonical files inside configured include
roots. The included file is compiled through the same frontend-to-IR path and
executed by the same VM. There is no second parser or ad hoc include evaluator.

`include_once` and `require_once` are tracked by canonical path in VM state.
Missing `include` emits a warning diagnostic and returns `false`; missing
`require` is fatal. `include_path`, stream wrappers, resources, arbitrary cwd
rules, and complete cross-file symbol redeclaration behavior are known gaps.

Fixture proof: `fixtures/runtime/valid/includes/include-return.php`,
`fixtures/runtime/valid/includes/share-variable.php`,
`fixtures/runtime/valid/includes/include-once.php`,
`fixtures/runtime/valid/includes/include-missing.php`, and
`fixtures/runtime/invalid/includes/require-missing.php`.

## Trace and Debugging

Tracing is off by default and is enabled with `VmOptions::trace` or
`php-vm run --trace`. Trace lines are deterministic and include step, function,
block, instruction kind, stack depth, output length, initialized locals, and
initialized registers. The trace avoids addresses and does not change stdout.

`php-vm dump-ir --with-source` prefixes deterministic IR dumps with numbered
source lines for debugging fixture failures.

Fixture proof: `fixtures/runtime/valid/variables/assignment.php`,
`nix develop -c cargo test -p php_vm trace`, and
`nix develop -c just vm-trace-smoke`.
