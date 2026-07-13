# json

- Priority: 17.6 promoted
- Selected manifest: `tests/phpt/manifests/modules/json.selected.jsonl`
- the selected close gate: 91 PASS, 3 SKIP, 0 FAIL, 0 BORK from 94 selected fixtures

## Scope

- `json_encode` scalar, list, object-map, simple object, and common flag basics
- `json_decode` associative-array and `stdClass` basics
- `json_last_error` and `json_last_error_msg`
- `JSON_THROW_ON_ERROR` failure routing
- `JsonException` protected/private debug shape for selected throw rows
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
- Debug-output-heavy `JsonSerializable` recursion rows that combine
  `__debugInfo`, `var_dump`, `print_r`, and `var_export`.
- All upstream `ext/json` rows that are currently target-green in the full
  target sweep, including decode error rows, invalid UTF-8 rows, U+2028/U+2029
  encoding, unsupported-type errors, and selected historical bug rows.

## Non-Scope

- Complete JSON flag parity beyond the upstream PHPT corpus

## Selected PHPT Fixtures

- `tests/phpt/generated/json/json-encode-basics.phpt`
- `tests/phpt/generated/json/json-encode-common-flags.phpt`
- `tests/phpt/generated/json/json-decode-basics.phpt`
- `tests/phpt/generated/json/json-encode-enums.phpt`
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
- `ext/json/tests/json_encode_exceptions.phpt`
- `ext/json/tests/json_decode_basic.phpt`
- `ext/json/tests/json_decode_exceptions.phpt`
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
  target sweep, for all 88 upstream rows total.

## Relevant Source Areas

- `crates/php_runtime/src/builtins/context.rs`
- `crates/php_runtime/src/builtins/modules/json.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`
- `nix develop -c just verify-phpt`

## Evidence

- Expanded the selected harness to 94 rows: 6 generated fixtures and all 88
  upstream `ext/json` rows.
- Added generated oracle fixtures for encode basics, encode common flags, decode
  basics, last-error state transitions, and `JSON_THROW_ON_ERROR`.
- Promoted upstream `ext/json/tests/001.phpt` through `009.phpt` after the
  target and reference probes both reached 9 PASS.
- Promoted upstream `json_validate_001.phpt` through `json_validate_005.phpt`,
  `json_encode_invalid_utf8.phpt`, and `json_exceptions_error_clearing.phpt`
  after focused target probes reached PASS.
- Latest full selected module gate after the full upstream corpus promotion,
  with reference and target reuse disabled:
  `PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`.
  Reference: 94 PASS. Target: 91 PASS / 3 SKIP / 0 FAIL / 0 BORK.
- The three target skips are prerequisite-gated rows, not JSON target
  mismatches: `ext/json/tests/bug41403.phpt` and
  `ext/json/tests/bug42785.phpt` require German locale support through
  `setlocale()`, and `ext/json/tests/gh15168.phpt` requires
  `zend.max_allowed_stack_size` support.
- Added a VM-backed `JsonSerializable` dispatch bridge for `json_encode`.
  Focused PHPT probes for `serialize.phpt`, `bug61978.phpt`, `bug66025.phpt`,
  `bug68992.phpt`, `bug71835.phpt`, `bug72069.phpt`, and `bug73113.phpt`
  reached PASS with reference and target reuse disabled.
- Latest selected module gate after full upstream-corpus promotion:
  target 91 PASS / 3 SKIP; 0 non-green outcomes.
- Added request-frame recursion tracking for nested `JsonSerializable`
  `json_encode($this)` calls so recursive re-entry reports
  `JSON_ERROR_RECURSION` instead of overflowing the Rust stack.
- Focused probes with target/reuse disabled reached PASS for
  `bug77843.phpt`, `json_encode_recursion_01.phpt`,
  `json_encode_recursion_02.phpt`, and `json_encode_recursion_06.phpt`.
- Added VM debug-output preparation for `print_r`, recursive `__debugInfo`
  recursion-marker handling for `var_dump`, and unrooted temporary handle
  release after VM-mediated `json_encode`.
- Focused probes with target/reuse disabled reached PASS for
  `json_encode_recursion_03.phpt`, `json_encode_recursion_04.phpt`, and
  `json_encode_recursion_05.phpt`.
- Added php-src-compatible protected/private storage for internal throwable
  debug output and attached failed `json_decode`/`json_encode` builtin frames
  before raising `JsonException`.
- Focused probes with target/reuse disabled reached PASS for
  `json_decode_exceptions.phpt` and `json_encode_exceptions.phpt`.
- Fixed PHP-visible object handle rooting for objects reachable through live VM
  registers, restoring `ext/json/tests/serialize.phpt` when `(object) $array`
  contains nested object values. Focused `serialize.phpt` and the full JSON
  module gate both reached 0 non-green outcomes with reference and target reuse
  disabled.

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
- Selected `JsonException` rows now dump the php-src-compatible
  protected/private `Exception` property layout, including builtin call traces.
- `JsonSerializable` dispatch is now bridged through the VM for selected rows,
  including userland return values, self-return public-property fallback,
  callback exceptions, nested self-encode recursion, and partial recursion
  substitution.
- Associative `json_decode` now normalizes canonical decimal object keys to
  PHP integer array keys while preserving non-canonical numeric strings such as
  `"012"`.
- The full upstream `ext/json` corpus is selected: 88 upstream rows plus 6
  generated fixtures.

## Next Step

Keep the full upstream `ext/json` corpus green while expanding JSON flag/API
parity beyond PHPT coverage.
