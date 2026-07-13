# Performance Cranelift Array ABI

Reference target: PHP 8.5.7 (`php-8.5.7`).

A later stage defines the read-only packed-array ABI surface used by later
Cranelift packed-array fast paths. It documents the current runtime layout and
keeps the JIT behind helper/accessor functions until safety work explicitly
allows lower-level access.

## Current Runtime Array Layout

Runtime arrays are represented as `Value::Array(PhpArray)` in `php_runtime`.
`PhpArray` is an opaque ordered-map facade with copy-on-write storage:

- `PhpArray` owns an `Rc<ArrayStorage>`.
- `ArrayStorage` currently stores insertion-ordered `Vec<ArrayEntry>` entries.
- Each `ArrayEntry` stores an `ArrayKey` plus a runtime `Value`.
- `ArrayKey` is either `Int(i64)` or `String(PhpString)` after PHP key
  normalization.
- `next_append_key` tracks PHP append semantics.
- `packed_len: Option<usize>` is metadata proving that keys are exactly
  `0..len` in insertion order.

The backing `Vec<ArrayEntry>` is intentionally not an ABI. JIT-generated code
must not read it directly in Performance. The only valid packed-array read surface
is the helper API below, which uses the public `PhpArray` facade.

## Layout Version And Guards

`php_runtime::PHP_JIT_ARRAY_LAYOUT_VERSION` is the stable Performance layout token
for this read-only helper ABI. `php_jit_array_layout_guard(version)` returns
true only when the caller-provided token matches the runtime token.

The packed-int read guard is `php_jit_array_is_packed_ints(value) -> status`.
It accepts only arrays that satisfy all of these constraints:

- the value is a `Value::Array`;
- `PhpArray::is_packed_fast()` proves contiguous integer keys;
- every element is a plain `Value::Int`;
- no element is a `Value::Reference`.

Shared COW storage is accepted because these helpers perform read-only fetches
through the public `PhpArray` facade and never expose mutable storage to
generated code. Reference elements are still rejected so reference-returning
dim access cannot be mistaken for an integer value fast path.

## Helper Surface

The runtime helper functions live in `crates/php_runtime/src/jit_array.rs`:

| Helper | Purpose |
| --- | --- |
| `php_jit_array_is_packed_ints(value) -> status` | Conservative layout, int-element, and alias guard. |
| `php_jit_array_len(value, out_len) -> status` | Writes packed length after the same guards. |
| `php_jit_array_fetch_int_slow(value, index, out_int) -> status` | Writes an integer element fetched through `PhpArray::packed_element_fast`. |

The helper status model is:

| Status | Meaning |
| ---: | --- |
| `PHP_JIT_ARRAY_STATUS_OK` (`0`) | Helper operation succeeded; out pointers are initialized where present. |
| `PHP_JIT_ARRAY_STATUS_FALLBACK` (`1`) | The interpreter must handle the access; out pointers must be ignored. |
| `PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT` (`2`) | The integer index is outside the packed array length. |
| `PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT` (`3`) | The value is not a packed int array, contains references, or otherwise misses the layout guard. |

The Rust helpers expose richer `PhpJitArrayAbiError` reasons for tests and
future diagnostics: `not_array`, `not_packed`, `aliased_or_referenced`,
`non_int_element`, and `out_of_bounds`.

`php_jit` records the helper symbols in the stable registry:

- `php_jit_array_is_packed_ints`
- `php_jit_array_len`
- `php_jit_array_fetch_int_slow`

that stage does not lower array access to Cranelift. a later stage adds
the first helper-assisted native fast path for `$xs[$i]` when `$xs` is a typed
array parameter and `$i` is a typed integer parameter. The generated code keeps
the read-only helper boundary, checks negative indexes before the helper call,
maps out-of-bounds to `PHP_JIT_ARRAY_STATUS_BOUNDS_EXIT`, maps layout misses to
`PHP_JIT_ARRAY_STATUS_LAYOUT_EXIT`, and falls back to the interpreter on every
non-OK status.

that stage reuses the same read-only helper boundary for packed foreach
integer reductions. The native loop calls the VM-owned packed length shim once,
then fetches each element through `php_jit_array_fetch_int_slow`; generated code
does not read `ArrayStorage.entries`, mutate array storage, or perform COW
writes. Layout, element-type, reference, and overflow failures side-exit before
the interpreter recomputes the result from the function entry.

## Property Load Helper ABI

that stage introduces the first object property fast path through a
helper boundary rather than direct object-layout access. The helper symbol is
`php_jit_property_load_monomorphic_fast` and its native entry shape is:

```text
(value_ptr: usize, metadata_ptr: usize, out_value_ptr: *mut usize) -> i32
```

`metadata_ptr` points to handle-owned `JitPropertyLoadMetadata`, including the
normalized receiver class, class id, property name, runtime storage name,
source-order property slot index, and class-layout version. Before native
entry, the VM checks receiver class id and layout version against the current
execution state. The helper rechecks receiver class identity, fetches through
the runtime object property API, rejects missing storage, and rejects
uninitialized typed properties. Successful helpers return a boxed cloned
`Value`; the VM immediately takes ownership and frees it after the native call.

Property-load statuses are:

| Status | Meaning |
| ---: | --- |
| `0` | Helper operation succeeded; output pointer receives a boxed `Value`. |
| `1` | Generic fallback; output pointer must be ignored. |
| `21` | Receiver class guard failed. |
| `22` | Class-layout version guard failed. |
| `23` | Typed property storage is uninitialized. |
| `24` | Expected property storage is missing. |

The Performance property-load ABI is read-only. It does not expose property writes,
dynamic properties, direct object-slot reads, hook dispatch, magic `__get`
dispatch, or visibility checks inside native code. Visibility, hooks, magic,
and declared-property shape are validated before compilation; unsupported cases
remain interpreter fallbacks or side-exit before interpreter re-entry.

## Fixtures

The Cranelift diff corpus includes packed and mixed examples:

- `tests/fixtures/performance/cranelift/arrays/packed-array-ints.php`
- `tests/fixtures/performance/cranelift/arrays/mixed-array-fallback.php`

At that stage both fixtures are expected to preserve output parity through
the existing fallback path. Native packed-array fetch counters are introduced
by that stage, which adds:

- `tests/fixtures/performance/cranelift/arrays/packed-fetch-valid.php`
- `tests/fixtures/performance/cranelift/arrays/packed-fetch-out-of-bounds.php`
- `tests/fixtures/performance/cranelift/arrays/packed-fetch-mixed-array.php`
- `tests/fixtures/performance/cranelift/arrays/packed-fetch-string-key.php`
- `tests/fixtures/performance/cranelift/arrays/packed-fetch-negative-index.php`

A later stage adds packed-foreach reduction fixtures:

- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-all-int.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-mixed-element.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-empty.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-large.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-by-ref-non-eligible.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-body-mutation-non-eligible.php`
- `tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-overflow.php`

## Safety Boundaries

- The ABI is read-only.
- No mutation or COW write path is exposed.
- By-reference foreach and reference-returning dim access are outside the
  native array fast paths.
- JIT code must not directly read `ArrayStorage.entries`.
- JIT code must not directly read object property storage in the 07.CL.26
  property-load path.
- Non-packed, reference-containing, non-int, and out-of-bounds cases must fall
  back cleanly.
- Shared COW array storage is safe only for these read-only helper calls; any
  future direct storage access or mutation path needs a new alias guard and
  safety audit.

## Validation

```bash
nix develop -c cargo test --workspace --features jit-cranelift
nix develop -c just jit-cranelift-diff
nix develop -c just cranelift-guard-report
```
