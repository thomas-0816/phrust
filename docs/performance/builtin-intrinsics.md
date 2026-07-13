# Builtin Intrinsics Ladder

This note defines the fastest-engine intrinsic ladder for hot PHP internal
functions. Intrinsics are exact fast paths over existing VM/runtime semantics,
not alternate standard-library implementations.

## Ranking Evidence

The current priority comes from committed counter surfaces and stdlib coverage:

| Rank | Candidate | Current evidence | FPE-25 status |
| ---: | --- | --- | --- |
| 1 | `strlen` | Existing exact fast stub, `stdlib_dispatch.php`, `inline-cache-smoke`, and `builtin_fast_stub_hits.strlen`. | Already covered by FPE-06; retained in the ladder. |
| 2 | `count` | Existing exact array fast path, packed-array counters, `stdlib_dispatch.php`, and `builtin_fast_stub_hits.count`. | Already covered by FPE-06 and array fast paths; retained in the ladder. |
| 3 | `strtolower` | `stdlib_dispatch.php`, `STDLIB_STRING_TRANSFORM`, runtime string builtin, and SIMD-capable byte-kernel ASCII lowercase facade. | Added as an exact string intrinsic. |
| 4 | `str_contains` | `STDLIB_STRING_SEARCH`, generated PHPT string coverage, and binary-safe runtime builtin. | Added as an exact two-string intrinsic. |
| 5 | `str_starts_with` | `STDLIB_STRING_SEARCH`, generated PHPT string coverage, and binary-safe runtime builtin. | Added as an exact two-string intrinsic. |
| 6 | `str_ends_with` | `STDLIB_STRING_SEARCH`, generated PHPT string coverage, and binary-safe runtime builtin. | Added as an exact two-string intrinsic. |
| 7 | `is_int`, `is_string`, `is_array` | Existing exact type predicate stubs and `inline-cache-smoke`. | Already covered by FPE-06; retained in the ladder. |
| 8 | `array_key_exists` | Standard array differential coverage exists, but key coercion and numeric-string ambiguity need a dedicated guard matrix. | Deferred. |
| 9 | `in_array` | Standard array differential coverage exists, but loose comparison, strict mode, arrays/objects, and conversion diagnostics are broad. | Deferred. |
| 10 | `explode`, `implode` | String transform differential coverage exists; the exact single-byte/no-limit `explode` runtime path now uses byte-kernel pre-count/search, but empty-separator `ValueError`, limits, and element conversion still need dedicated counters before a VM intrinsic. | Deferred. |
| 11 | `isset` / `empty` | Modeled as language constructs/ops rather than generic internal functions. | Belongs in opcode/lowering work, not builtin stubs. |
| 12 | `json_encode` | JSON differential coverage exists, but flags, depth, request-local last-error state, objects, and `JsonSerializable` gaps make it stateful. | Deferred. |
| 13 | `htmlspecialchars` | URL/HTML differential coverage exists; the default flags/double-encode runtime path now uses the byte-kernel HTML escape pre-scan, but non-default flags, encoding policy, double-encode variants, and diagnostics are wider than a safe VM intrinsic. | Deferred. |
| 14 | `file_exists`, `is_file`, `realpath` | Filesystem coverage exists, but cwd, include path, open_basedir, stream wrappers, stat cache, and mutation invalidation make these path-semantics sensitive. | Deferred. |
| 15 | `is_numeric` | Exact `mixed`→`bool` predicate reusing `php_runtime::numeric_string::classify_php_string` (`IntString`/`FloatString`); reference args fall back as `by_ref`. VM test asserts behavior parity with the generic builtin and a non-zero `intrinsic_hits.is_numeric`; the `trivial-predicate-intrinsics.php` fixture adds numeric-string and leading-numeric differential cases. | Added as an exact predicate intrinsic. |

> The table above is the original FPE-25 ladder and its per-row rationale. The
> intrinsic set has since grown well beyond it; the authoritative current set is
> the `BUILTIN_INTRINSICS` table in `crates/php_vm/src/vm/mod.rs`. Several rows
> once marked *Deferred* are now implemented (e.g. `array_key_exists`,
> `in_array`, single-byte `explode`, `implode`, default `htmlspecialchars`),
> alongside the `is_bool`/`is_float`/`is_null`/`is_object`/`is_scalar`/
> `is_numeric` predicates, `current`/`key`, `array_keys`, `substr`, `trim`, and
> `str_replace`. Each still passes the same exact-shape guards below.

## Eligibility

An intrinsic candidate can take the fast path only when all guards are true:

- The builtin name is one of the explicitly supported intrinsic names.
- Arity is exact for the intrinsic shape.
- Named arguments have already passed normal call binding; the intrinsic does
  not reinterpret names or metadata.
- Argument values have the exact expected runtime shape.
- No argument is represented as a runtime reference value.
- The fast result is byte-for-byte equivalent to the existing generic builtin
  for that exact shape.
- `TypeError`, `ValueError`, arity diagnostics, warnings, output, request state,
  and reflection metadata remain owned by the generic path.

Any failed guard records an intrinsic miss and `intrinsic_fallback_by_reason`
before falling back to the existing builtin registry execution path.

## Implemented Ladder

FPE-25 keeps the ladder request-local and interpreter-side:

1. Generic builtin call through `BuiltinRegistry`.
2. Function/builtin inline-cache lookup when `--inline-caches=on`.
3. Exact interpreter intrinsic stub for:
   - `strlen(string)`
   - `count(array)`
   - `is_int(value)`
   - `is_string(value)`
   - `is_array(value)`
   - `strtolower(string)`
   - `str_contains(string, string)`
   - `str_starts_with(string, string)`
   - `str_ends_with(string, string)`
4. Byte-kernel facade for ASCII lowercase copies through
   `php_runtime::builtins::string_intrinsics::strtolower_ascii` and
   `php_source::byte_kernel`; string predicate stubs stay byte-slice based and
   binary-safe. Runtime string helpers also share SIMD-capable
   uppercase/lowercase, HTML escape-scan, single-byte `explode`, default-trim,
   forward/reverse substring search, ASCII-folded search, digit/whitespace
   classification, and delimiter-scan kernels, but those are not separate VM
   builtin stubs yet.
5. Specialized bytecode and native stubs remain future work.

No new specialized bytecode form is added for these FPE-25 intrinsics. The
counter `specialized_builtin_opcode_hits` is emitted for future bytecode forms
and remains empty until a bytecode lowering has generic-call parity fixtures.

## Counters

The VM emits the older compatibility counters plus FPE-25 counters:

- `builtin_fast_stub_hits`
- `builtin_fast_stub_misses`
- `builtin_fast_stub_fallback_by_reason`
- `builtin_intrinsic_candidates`
- `intrinsic_hits`
- `intrinsic_misses`
- `intrinsic_fallback_by_reason`
- `specialized_builtin_opcode_hits`

`inline-cache-smoke` requires `strtolower`, `str_contains`,
`str_starts_with`, and `str_ends_with` intrinsic hits while
`--inline-caches=on`, and requires no intrinsic counters while inline caches are
off.

## Correctness Fixtures

`tests/fixtures/stdlib/_harness/stdlib/builtin_intrinsics.php` covers each new
intrinsic with exact hits and generic fallbacks:

- wrong arity;
- wrong type;
- named arguments;
- reference-backed variables;
- catchable error class shape;
- binary strings;
- large inputs;
- empty-string edge values.

The fixture is included in `just diff-stdlib` and therefore in
`just verify-stdlib`.
