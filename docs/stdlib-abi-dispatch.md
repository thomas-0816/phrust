# Standard-library builtin dispatch

The VM has one preferred path for PHP standard-library functions:

1. `builtin_function_call_target()` resolves names present in
   `php_runtime::BuiltinRegistry` to `FunctionCallBuiltinKind::InternalRegistry`.
2. `execute_internal_registry_builtin()` applies generated `php_std` arginfo
   validation and VM string coercion at the call site.
3. `execute_builtin_entry()` runs the registered builtin with the executing VM
   request context, request-local state, output buffer, and call source span.

New PHP standard-library functions should be added to the registry and generated
arginfo metadata, then reached through `InternalRegistry`. They should not add a
parallel VM dispatch path unless they require VM-only services that the registry
cannot receive directly.

The remaining VM-owned builtin dispatch groups are quarantined exceptions:

- `AutoloadOrSymbolIntrospection`: reads VM dynamic function/class state and
  autoload stacks.
- `Config`: mutates request ini state before the registry owns the same request
  plumbing.
- `ErrorHandling`: needs VM stack traces, throwables, and userland callbacks.
- `OutputBuffering`: operates on the VM output-buffer stack.
- `Environment`: owns superglobal and request seeding behavior.
- `Process`: enforces deterministic process capability policy and by-reference
  status arguments.
- `PcreCallback`: invokes userland callbacks during regex replacement.
- `ArrayCallback`: invokes userland callbacks for array walkers and filters.
- `ArraySort`: mutates by-reference array arguments and invokes comparators.

`php_std::abi` is the compatibility boundary for stdlib ABI tests and new
adapter work: it carries call arguments, by-reference cells, request metadata,
output, non-fatal diagnostics, fatal diagnostics, return values, and source
spans. The VM bridge must receive the executing request/output context from its
caller; it must not manufacture placeholder request state when a real VM
request exists.
