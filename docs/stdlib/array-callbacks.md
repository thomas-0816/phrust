# Standard library Array Callback Functions

The standard library implements the PHP-visible array callback helper MVP on top of the
existing VM callable dispatch path:

- `array_map`
- `array_filter`
- `array_reduce`
- `array_walk`
- `array_any`
- `array_all`
- `array_find`
- `array_find_key`

The implementation is VM-routed because these builtins must invoke user
functions, closures, internal builtins, object callables, and static method
callables. The runtime builtin registry still exposes the function names for
introspection and first-class callable resolution, but direct execution is
handled by `php_vm`.

Covered behavior:

- callback errors propagate through the normal VM runtime-error path
- `array_map` preserves keys for a single input array and reindexes multi-array
  results
- `array_filter` preserves original keys and supports value, key, and
  value/key callback modes
- `array_reduce` carries the accumulator through callback returns
- `array_walk` invokes callbacks with value, key, and optional userdata
- PHP 8.5 predicate helpers return the first matching value/key or boolean
  result as appropriate

Known gap:

- `STDLIB-GAP-ARRAY-WALK-BY-REF-MUTATION`: `array_walk` does not yet pass
  element references into callbacks that declare `&$value`.

Validation:

- `nix develop -c cargo test -p php_vm array_callback_builtins_execute_php_callables`
- `nix develop -c just diff-stdlib`
- `nix develop -c just performance-tests`
- `nix develop -c just verify-stdlib`
