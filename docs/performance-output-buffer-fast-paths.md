# Performance Output Buffer Fast Paths

Work item keeps PHP-visible output semantics unchanged while reducing
avoidable work in string-heavy output paths.

## Implemented Paths

- `OutputBuffer` tracks request-local append, batch-write, flush, fast-append,
  and generic fallback statistics.
- Empty writes are ignored before touching the active buffer.
- Internal callers can append several byte slices through one active-buffer
  reservation with `write_slices`.
- The VM preallocates the root output buffer from statically known literal
  `echo` string operands. This is only a reserve hint and does not write bytes.
- `echo` writes exact `Value::String`, `true`, and integer values through
  `write_fast_*` append APIs. `false` and `null` remain zero-byte writes.
- `output_fast_appends` counts VM-proven exact-output appends. The count is
  intentionally a path counter, not a byte count.
- `output_slow_appends_by_reason` records stable generic fallback reasons such
  as `reference_deref`, `float_conversion`, `array_conversion_warning`,
  `object_to_string`, `resource_conversion`, and callable/uninitialized
  conversion errors.
- Object, reference, float, resource, array, callable, generator, fiber, and
  fallback conversions still flow through `value_to_string`, preserving
  `__toString` side effects and conversion errors.
- Existing dense-bytecode superinstructions keep producer-plus-echo batching
  limited to already verified `load_const_echo`, `load_local_echo`, and
  `binary_concat_echo` shapes. Multi-argument IR `echo` still executes in source
  order unless that bytecode selection proves an unchanged fused operation.
- Exact string/string concat quickening is guarded on both operands being
  `Value::String`, preallocates the combined byte capacity when the checked size
  fits, and records `string_concat_fast_path_hits`/misses.

## Semantics Boundaries

Output buffering levels are not bypassed. Writes still target the active buffer
when `ob_start` is active, and `ob_end_flush`/shutdown flushing still controls
when nested bytes become visible in root output.

Output buffering callbacks remain the Standard library unsupported gap. Fast paths must
not attempt to invoke or skip callbacks.

## Validation

The focused VM tests cover multi-argument `echo`, `print` return value output,
buffer flushing, object `__toString` fallback, throwing `__toString`, output
fast-append counters, slow fallback reasons, and the callback unsupported
diagnostic. The Performance smoke corpus includes
`tests/fixtures/performance/perf_smoke/output_writes.php` and
`tests/fixtures/performance/perf_smoke/output_scalar_fast_paths.php` so
`benchmark-smoke` reports output append/flush/fast-append counters in the
hotpath inventory and performance report.
