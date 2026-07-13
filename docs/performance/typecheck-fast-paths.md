# Performance Typecheck Fast Paths

The performance layer provides conservative fast paths for runtime parameter, return, and
property type checks.

## Covered Checks

The VM typecheck entry points are:

- parameter prologue checks after argument binding and weak-mode coercion,
- return type checks on normal returns and returns through `finally`,
- typed instance and static property writes.

Fast paths are enabled by `VmOptions::typecheck_fast_paths`, which defaults to
`true`. Tests run the same fixtures with the option enabled and disabled.

## Fast Path Contract

The fast path only accepts values that already satisfy a declaration:

- exact scalar/runtime facades: `int`, `string`, `bool`, `float`, `array`,
  `object`, and `callable`,
- nullable simple types when the value is `null` or the inner simple type is an
  exact match,
- exact class names, including internal `Fiber` and `Generator` names.

It never fast-rejects. Any non-accepted value records a miss and falls back to
the existing generic matcher, which preserves inheritance, interface, internal
class, union, intersection, and DNF behavior.

Weak parameter coercion still runs before fast matching. By-reference arguments
continue to skip weak coercion. Typed variadic parameters validate and coerce
each collected variadic array entry before the parameter local is initialized.

## Counters

When VM counters are enabled, the fast path records:

- `typecheck_fast_path_hits` for accepted exact matches,
- `typecheck_fast_path_misses` for fallback to the generic matcher.

With `VmOptions::typecheck_fast_paths = false`, both counters remain zero while
the old generic matcher handles every check.
