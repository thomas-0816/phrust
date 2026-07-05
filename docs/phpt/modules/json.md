# json

- Priority: 17.6 promoted
- Selected manifest: `tests/phpt/manifests/modules/json.selected.jsonl`
- the selected close gate: 83 PASS, 3 SKIP, 0 FAIL, 0 BORK from 86 selected fixtures

## Scope

- `json_encode` scalar, list, object-map, simple object, and common flag basics
- `json_decode` associative-array and `stdClass` basics
- `json_last_error` and `json_last_error_msg`
- `JSON_THROW_ON_ERROR` failure routing
- `JSON_FORCE_OBJECT`, default Unicode escaping, decode depth/state/control
  diagnostics, `JSON_BIGINT_AS_STRING`, and recursive encode diagnostics
- `json_validate` general usage, argument errors, depth errors, invalid UTF-8,
  and parity against selected `json_decode` inputs
- `json_encode` invalid UTF-8 ignore/substitute modes and selected
  `JSON_THROW_ON_ERROR` error-clearing behavior
- Selected `JsonSerializable` userland dispatch rows, including self-return,
  exception propagation, nested encode, and partial recursion behavior.
- Selected mutation and recursion-sensitive `JsonSerializable` rows where
  nested self-encoding reports `JSON_ERROR_RECURSION` without crashing.
- All upstream `ext/json` rows that are currently target-green in the full
  target sweep, including decode error rows, invalid UTF-8 rows, U+2028/U+2029
  encoding, unsupported-type errors, and selected historical bug rows.

## Non-Scope

- Exact `JsonException` debug/var_dump shape parity.
- Remaining debug-output-heavy `JsonSerializable` recursion fixtures.
- Complete JSON flag parity beyond the promoted upstream rows

## Selected PHPT Fixtures

- `tests/phpt/generated/json/json-encode-basics.phpt`
- `tests/phpt/generated/json/json-encode-common-flags.phpt`
- `tests/phpt/generated/json/json-decode-basics.phpt`
- `tests/phpt/generated/json/json-last-error-state.phpt`
- `tests/phpt/generated/json/json-throw-on-error.phpt`
- `ext/json/tests/001.phpt`
- `ext/json/tests/002.phpt`
- `ext/json/tests/003.phpt`
- `ext/json/tests/004.phpt`
- `ext/json/tests/005.phpt`
- `ext/json/tests/006.phpt`
- `ext/json/tests/007.phpt`
- `ext/json/tests/008.phpt`
- `ext/json/tests/009.phpt`
- `ext/json/tests/json_encode_basic.phpt`
- `ext/json/tests/json_decode_basic.phpt`
- `ext/json/tests/json_last_error_error.phpt`
- `ext/json/tests/json_last_error_msg_error.phpt`
- `ext/json/tests/json_encode_unescaped_slashes.phpt`
- `ext/json/tests/json_encode_pretty_print.phpt`
- `ext/json/tests/json_encode_numeric.phpt`
- `ext/json/tests/pass002.phpt`
- `ext/json/tests/pass003.phpt`
- `ext/json/tests/json_encode_pretty_print2.phpt`
- `ext/json/tests/json_validate_001.phpt`
- `ext/json/tests/json_validate_002.phpt`
- `ext/json/tests/json_validate_003.phpt`
- `ext/json/tests/json_validate_004.phpt`
- `ext/json/tests/json_validate_005.phpt`
- `ext/json/tests/json_encode_invalid_utf8.phpt`
- `ext/json/tests/json_exceptions_error_clearing.phpt`
- `ext/json/tests/bug61978.phpt`
- `ext/json/tests/bug66025.phpt`
- `ext/json/tests/bug68992.phpt`
- `ext/json/tests/bug71835.phpt`
- `ext/json/tests/bug72069.phpt`
- `ext/json/tests/bug73113.phpt`
- `ext/json/tests/serialize.phpt`
- all additional target-green upstream rows from the latest full `ext/json`
  target sweep, for 81 selected upstream rows total.

## Relevant Source Areas

- `crates/php_runtime/src/builtins/context.rs`
- `crates/php_runtime/src/builtins/modules/json.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`
- `nix develop -c just verify-phpt`

## Evidence

- Expanded the selected harness from 31 rows to 75 rows: 5 generated fixtures,
  67 upstream PASS rows, and 3 upstream SKIP rows.
- Added generated oracle fixtures for encode basics, encode common flags, decode
  basics, last-error state transitions, and `JSON_THROW_ON_ERROR`.
- Promoted upstream `ext/json/tests/001.phpt` through `009.phpt` after the
  target and reference probes both reached 9 PASS.
- Promoted upstream `json_validate_001.phpt` through `json_validate_005.phpt`,
  `json_encode_invalid_utf8.phpt`, and `json_exceptions_error_clearing.phpt`
  after focused target probes reached PASS.
- Latest full upstream target sweep before promotion: 67 PASS, 3 SKIP, 18 FAIL.
- Latest selected module gate after promotion, with reference reuse disabled:
  target 72 PASS / 3 SKIP; reference 75 PASS; 0 non-green outcomes.
- Added a VM-backed `JsonSerializable` dispatch bridge for `json_encode`.
  Focused PHPT probes for `serialize.phpt`, `bug61978.phpt`, `bug66025.phpt`,
  `bug68992.phpt`, `bug71835.phpt`, `bug72069.phpt`, and `bug73113.phpt`
  reached PASS with reference and target reuse disabled.
- Latest selected module gate after `JsonSerializable` promotion, with reuse
  disabled: target 83 PASS / 3 SKIP; reference 86 PASS; 0 non-green outcomes.
- Added request-frame recursion tracking for nested `JsonSerializable`
  `json_encode($this)` calls so recursive re-entry reports
  `JSON_ERROR_RECURSION` instead of overflowing the Rust stack.
- Focused probes with target/reuse disabled reached PASS for
  `bug77843.phpt`, `json_encode_recursion_01.phpt`,
  `json_encode_recursion_02.phpt`, and `json_encode_recursion_06.phpt`.

## Known Gaps

- Request-local JSON last-error state now persists across VM builtin calls.
- `json_encode` now matches the selected scalar/list/map/simple-object, common
  flag, slash escaping, pretty-print, numeric-check, and insertion-order PHPTs.
- `json_encode` now detects selected recursive array/object graphs and reports
  `JSON_ERROR_RECURSION`, including partial-output `null` substitution.
- `json_decode` now enforces selected depth failures, preserves selected big
  integers with `JSON_BIGINT_AS_STRING`, and distinguishes selected state
  mismatch and control-character diagnostics.
- `json_validate` now matches selected upstream general, error, depth, invalid
  UTF-8, and decode-comparison rows.
- `json_encode` now matches selected upstream invalid UTF-8 ignore and
  substitute rows.
- `JSON_THROW_ON_ERROR` decode failures now route to catchable `JsonException`
  through the existing VM throwable path and preserve selected last-error
  clearing semantics.
- `JsonSerializable` dispatch is now bridged through the VM for selected rows,
  including userland return values, self-return public-property fallback,
  callback exceptions, nested self-encode recursion, and partial recursion
  substitution.
- Remaining upstream failures are now narrowed to 7 rows:
  `json_decode_exceptions.phpt`, `json_encode_exceptions.phpt`,
  `json_encode_recursion_03.phpt` through `json_encode_recursion_05.phpt`,
  `pass001.1.phpt`, and `pass001.1_64bit.phpt`.

## Next Step

Close the remaining debug-output-heavy `JsonSerializable` recursion,
`JsonException` debug-shape, and `pass001` object-id rows, then rerun the full
upstream sweep.
