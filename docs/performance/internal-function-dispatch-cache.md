# Performance Internal Function Dispatch Cache

The performance layer provides a request-local VM cache for generic internal builtin dispatch
metadata. The cache stores the runtime builtin registry entry for hot standard
library names after the first lookup:

- `count`
- `strlen`
- `is_*`
- selected array and string helpers when they exist in `BuiltinRegistry`

The cache only avoids repeated registry lookup for dispatch metadata. It does
not change `function_exists`, reflection metadata, named-argument conversion,
arity checks, type checks, or builtin `ValueError` diagnostics.

The VM option `VmOptions::internal_function_dispatch_cache` is enabled by
default and can be disabled for A/B tests. Counters are emitted when
`collect_counters` is enabled:

- `internal_function_dispatches`
- `internal_function_dispatch_cache_hits`
- `internal_function_dispatch_cache_misses`
- `internal_count_array_direct_fast_path_hits`
- `builtin_intrinsic_candidates`
- `intrinsic_hits`
- `intrinsic_misses`
- `intrinsic_fallback_by_reason`

The semantic fast paths are the conservative `count(array)` direct case and the
exact intrinsic set documented in `docs/performance/builtin-intrinsics.md`.
They run after builtin arguments have been normalized to positional values and
only for exact non-reference shapes. Wrong arity, non-array values, recursive
mode, objects, references that do not resolve to arrays, coercion-sensitive
values, and stateful builtins fall back to the existing builtin handler.

The Performance smoke benchmark fixture `stdlib_dispatch.php` exercises repeated
`count`, `strlen`, `is_int`, `array_values`, `strtolower`,
`str_contains`, `str_starts_with`, and `str_ends_with` calls so
`target/performance/hotpath-inventory.md` can report visible dispatch-cache hits from the
standard-library-heavy path.
