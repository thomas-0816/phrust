# Runtime VM Structure

The VM crate keeps the public `php_vm::vm` API stable while splitting the
implementation into a module directory:

- `crates/php_vm/src/vm/mod.rs` is the temporary migration facade for public
  exports, construction, request lifecycle, and top-level orchestration. The
  implementation ownership and dependency rules are defined by the
  [VM decomposition map](../architecture/vm-decomposition.md).
- `crates/php_vm/src/vm/prelude.rs` owns private VM implementation imports for
  VM submodules. It is not part of the public API surface.
- `crates/php_vm/src/vm/options.rs` owns `VmOptions` and the public execution
  mode enums re-exported from `php_vm::vm`.
- `crates/php_vm/src/vm/result.rs` owns `VmResult`, `VmControlFlow`, and
  `VmResult` constructor helpers.
- `crates/php_vm/src/vm/arguments.rs` owns user-function argument preparation.
- `crates/php_vm/src/vm/dense_method_dispatch.rs` owns dense bytecode method
  call dispatch helpers.
- `crates/php_vm/src/vm/generator_fiber.rs` owns generator and fiber runtime
  method handling.

The module split does not add Zend, function, callable, object, or
standard-library behavior. New VM behavior continues to enter through the
existing frontend-to-IR-to-VM pipeline. Implementation must follow the target
ownership map rather than adding new behavior to the migration facade.

`scripts/verify/source_integrity.py` pins the expected VM module wiring, the
non-empty Rust source rule, and the `VmResult` helper ownership. It also rejects
direct `use super::*` imports from focused VM submodules so broad parent-module
reach-through is visible in one private prelude instead of being duplicated.
