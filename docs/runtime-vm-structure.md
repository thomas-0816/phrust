# Runtime VM Structure

The VM crate keeps the public `php_vm::vm` API stable while splitting the
implementation into a module directory:

- `crates/php_vm/src/vm/mod.rs` owns the interpreter state, dispatch loop, call
  execution, object integration, builtins, include handling, and tests.
- `crates/php_vm/src/vm/options.rs` owns `VmOptions` and the public execution
  mode enums re-exported from `php_vm::vm`.
- `crates/php_vm/src/vm/result.rs` owns `VmResult` and `VmControlFlow`.
- `crates/php_vm/src/vm/arguments.rs` owns user-function argument preparation.

This split is structural only. It does not add Zend, function, callable, object,
or standard-library behavior. New VM behavior should continue to enter through
the existing frontend-to-IR-to-VM pipeline and should move into focused VM
submodules only when that reduces ownership ambiguity.
