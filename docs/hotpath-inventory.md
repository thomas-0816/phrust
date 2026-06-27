# performance Hot-Path Inventory

Source report: `target/performance/benchmark-smoke.json`.

This inventory is derived from Rust VM counters in the performance smoke benchmark report. It uses counter totals, not wall-clock timings, to avoid host-specific priorities.

## Category Summary

| Category | Counter(s) | Total | Top fixture | Coverage |
| --- | --- | ---: | --- | --- |
| Dispatch | `instructions_executed` | 2319 | `tests/fixtures/performance/perf_smoke/arrays_packed.php` (459) | complete_for_current_counter_set |
| Calls | `function_calls, method_calls` | 81 | `tests/fixtures/performance/perf_smoke/stdlib_dispatch.php` (56) | complete_for_current_counter_set |
| Arrays | `array_dim_fetches` | 15 | `tests/fixtures/performance/perf_smoke/arrays_packed.php` (12) | complete_for_current_counter_set |
| Properties | `property_accesses, property_fetches` | 44 | `tests/fixtures/performance/perf_smoke/properties.php` (24) | complete_for_current_counter_set |
| Strings | `string_concats` | 13 | `tests/fixtures/performance/perf_smoke/strings_concat.php` (12) | complete_for_current_counter_set |
| Output | `output_bytes, output_buffer_appends, output_buffer_flushes, output_fast_appends` | 700 | `tests/fixtures/performance/perf_smoke/output_scalar_fast_paths.php` (273) | complete_for_current_counter_set |
| Type Checks | `type_checks` | 0 | none observed | no_events_in_smoke_corpus |
| Includes/Autoload | `includes, autoloads` | 2 | `tests/fixtures/performance/perf_smoke/autoload_smoke.php` (2) | complete_for_current_counter_set |
| Standard Library Calls | `internal_function_dispatches, internal_function_dispatch_cache_hits, internal_function_dispatch_cache_misses` | 59 | `tests/fixtures/performance/perf_smoke/stdlib_dispatch.php` (56) | builtin_dispatch_counters_visible_in_smoke_corpus |

## Prioritized Candidates

| Priority | Hot path | Evidence | Optimization layer | Risk | Required correctness tests | Benefit |
| ---: | --- | --- | --- | --- | --- | --- |
| 1 | Dispatch | 459 counted event(s) in tests/fixtures/performance/perf_smoke/arrays_packed.php | interpreter dispatch and bytecode layout | changing dispatch can reorder side effects or diagnostics | runtime fixture diff plus bytecode snapshots for the same fixture family | high |
| 2 | Output | 273 counted event(s) in tests/fixtures/performance/perf_smoke/output_scalar_fast_paths.php | echo/print output buffering and batched internal buffer appends | stdout/stderr bytes, output buffering levels, callbacks, and conversion errors are observable | echo, print, output-buffering, object-to-string, and conversion-error fixtures | high |
| 3 | Calls | 56 counted event(s) in tests/fixtures/performance/perf_smoke/stdlib_dispatch.php | call frame setup, method lookup, and later inline caches | call semantics include references, late static binding, visibility, and argument coercion | function, method, reference, variadic, and visibility fixtures | high |
| 4 | Standard Library Calls | 56 internal dispatch(es), 51 dispatch-cache hit(s), and 5 miss(es) in tests/fixtures/performance/perf_smoke/stdlib_dispatch.php covering count, strlen, is_int, array_values, strtolower | builtin dispatch and standard-library call shims | cache must not bypass named-argument conversion, arity checks, TypeError/ValueError diagnostics, or reflection metadata | per-builtin fixtures plus differential stdlib gates before any fast path | high |
| 5 | Properties | 24 counted event(s) in tests/fixtures/performance/perf_smoke/properties.php | property lookup and object layout caches | visibility, magic methods, dynamic properties, and typed properties are observable | public/private/protected, magic, dynamic, and typed-property fixtures | high |
| 6 | Arrays | 12 counted event(s) in tests/fixtures/performance/perf_smoke/arrays_packed.php | array dimension read/write fast paths | PHP array ordering, key coercion, references, and copy-on-write are observable | packed, mixed, append, foreach, reference, and copy-on-write fixtures | high |
| 7 | Strings | 12 counted event(s) in tests/fixtures/performance/perf_smoke/strings_concat.php | string concatenation allocation and conversion fast paths | PHP scalar conversion and binary-safe string behavior must stay exact | concat, scalar conversion, encoding-neutral, and error-order fixtures | high |
| 8 | Includes/Autoload | 2 counted event(s) in tests/fixtures/performance/perf_smoke/autoload_smoke.php | include path resolution and autoload metadata caches | include side effects, working directory, once semantics, and autoload order are observable | include/require, include_once/require_once, path, and autoload fixtures | low |

## Counter Gaps

- `PERF-GAP-HOTPATH-TYPE_CHECKS-NO-EVENTS`: No performance smoke fixture currently emits Type Checks counter events; the category is listed but not prioritized.
- `PERF-GAP-HOTPATH-CORPUS-REPRESENTATIVENESS`: The current smoke corpus and optional framework micro-smokes are deterministic but too small to represent real Composer/framework workloads.

## Non-Representative Fixture Notes

- The smoke corpus uses tiny deterministic loops so instruction counts are useful for ranking within the corpus, not for real-world throughput claims.
- No fixture exercises real Composer autoload trees, large arrays, I/O-heavy includes, closures with captures, generators, fibers, or exception-heavy paths at framework scale.
- Wall-clock timings are intentionally excluded from the hot-path priority calculation.

## No-Go Areas

- Do not change PHP-visible evaluation order, diagnostics, include side effects, or autoload ordering for a performance win.
- Do not implement JIT, standard-library ABI shortcuts, or semantic rewritesfrom this inventory.
- Do not promote a candidate without differential correctness fixtures for its risk area.
