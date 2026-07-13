# Performance Output Buffer Fast Paths

The performance layer keeps PHP-visible output semantics unchanged while reducing
avoidable work in string-heavy output paths.

## Implemented Paths

- `OutputBuffer` tracks request-local append, batch-write, batched-append,
  batch-byte, flush, fast-append, and generic fallback statistics.
- Empty writes are ignored before touching the active buffer.
- Internal callers can append several byte slices through one active-buffer
  reservation with `write_slices`.
- The IR VM batches adjacent exact-output `Echo` operations and `LoadConst` plus
  immediately following `Echo` producer pairs when every consumed value is a
  direct string, integer, `true`, `false`, or `null`.
- The VM preallocates the root output buffer from statically known literal
  `echo` string operands. This is only a reserve hint and does not write bytes.
- `echo` writes exact `Value::String`, `true`, and integer values through
  `write_fast_*` append APIs. `false` and `null` remain zero-byte writes.
- `output_fast_appends` counts VM-proven exact-output appends. The count is
  intentionally a path counter, not a byte count.
- `output_batched_appends` and `output_batch_bytes` report coalesced exact
  output writes; the legacy `output_buffer_batch_writes` counter remains for
  existing reports.
- `output_slow_appends_by_reason` records stable generic fallback reasons such
  as `reference_deref`, `float_conversion`, `array_conversion_warning`,
  `object_to_string`, `resource_conversion`, and callable/uninitialized
  conversion errors.
- Object, reference, float, resource, array, callable, generator, fiber, and
  fallback conversions still flow through `value_to_string`, preserving
  `__toString` side effects and conversion errors.
- Existing dense-bytecode superinstructions keep producer-plus-echo fusion
  limited to already verified `load_const_echo`, `load_local_echo`, and
  `binary_concat_echo` shapes. The IR adjacent-output batcher is separate and
  only crosses PHP-invisible constant-producer boundaries.
- Exact string/string concat quickening is guarded on both operands being
  `Value::String`, preallocates the combined byte capacity when the checked size
  fits, and records `string_concat_fast_path_hits`/misses.
- Generic concat now reserves the exact combined byte capacity after normal PHP
  conversions succeed, records `concat_prealloc_hits`, and attributes non-direct
  string/string operands in `concat_fallback_by_reason`.

## Semantics Boundaries

Output buffering levels are not bypassed. Writes still target the active buffer
when `ob_start` is active, and `ob_end_flush`/shutdown flushing still controls
when nested bytes become visible in root output.

Output buffering callbacks remain the Standard library unsupported gap. Fast paths must
not attempt to invoke or skip callbacks.

The output batcher stops before references, arrays, resources, objects,
callables, uninitialized values, diagnostics, calls, branches, buffer-control
builtins, and any other instruction that could expose conversion, flush, or
side-effect order.

## Validation

The focused VM tests cover multi-argument `echo`, `print` return value output,
buffer flushing, object `__toString` fallback, throwing `__toString`, output
fast-append counters, batched appends/bytes, nested-buffer batching, binary
strings, array/resource conversion fallback reasons, slow fallback reasons, and
the callback unsupported diagnostic. The Performance smoke corpus includes
`tests/fixtures/performance/perf_smoke/output_writes.php` and
`tests/fixtures/performance/perf_smoke/output_scalar_fast_paths.php` plus
`tests/fixtures/performance/perf_smoke/output_batching_v2.php` so
`benchmark-smoke`, framework smoke, the acceleration matrix, and performance
reports expose output append/flush/fast-append/batch counters.
