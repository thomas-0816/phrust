# json Current Focus Report

Generated from:

- `nix develop -c just phpt-dev-build`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`

Current focused target run:

| Outcome | Count |
| --- | ---: |
| PASS | 10 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

The focused reference run is green for all 10 selected PHPTs.

## Passing Fixtures

- `tests/phpt/generated/json/json-encode-basics.phpt`
- `tests/phpt/generated/json/json-encode-common-flags.phpt`
- `tests/phpt/generated/json/json-decode-basics.phpt`
- `tests/phpt/generated/json/json-last-error-state.phpt`
- `tests/phpt/generated/json/json-throw-on-error.phpt`
- `ext/json/tests/json_encode_basic.phpt`
- `ext/json/tests/json_decode_basic.phpt`
- `ext/json/tests/json_last_error_error.phpt`
- `ext/json/tests/json_last_error_msg_error.phpt`
- `ext/json/tests/json_encode_unescaped_slashes.phpt`

## Blockers

No blockers remain in the selected 10-test JSON PHPT harness.

## Prompt 17.6 Close

Close gates passed:

- `nix develop -c just diff-json-pcre-date`: PASS, total 3, pass 3,
  fail 0, skip 0, known_gap 0.
- `nix develop -c cargo test -p php_vm`: PASS, 345 tests.
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`:
  PASS, reference 10/10 and target 10/10.
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c just verify-stdlib`:
  PASS. The stdlib diff gates remained green, including
  `diff-json-pcre-date`.
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression`:
  PASS against the accepted baseline. The full run covered 21,548 tests with
  BORK 140, FAIL 9,680, PASS 2,654, SKIP 9,070, and XFAIL 4. The run
  directory was `target/phpt-work/full-runs/20260628T011951Z`.

The broad non-green full-regression counts match the accepted PHPT baseline and
do not leave a JSON-module blocker.

## Prompt 17.3 JSON Encode

`json_encode` now preserves PHP insertion order for arrays and simple objects,
escapes `/` by default, honors `JSON_UNESCAPED_SLASHES`, and normalizes
pretty-print indentation to PHP's four-space output shape. The selected upstream
`json_encode_basic.phpt`, `json_decode_basic.phpt`, and
`json_encode_unescaped_slashes.phpt` fixtures now pass.

## Prompt 17.4 JSON Decode

`json_decode` success and failure paths now preserve request-local JSON error
state and `JSON_THROW_ON_ERROR` failures route through a catchable
`JsonException`. `JsonException` is included in the existing internal throwable
hierarchy as an `Exception` subclass rather than using a separate JSON-only
catch path.

## Prompt 17.5 JsonSerializable

`JsonSerializable` dispatch remains a documented known gap. `json_encode` is
implemented in the runtime builtin layer, while userland method invocation is
owned by the VM call path. There is no clean `BuiltinContext` bridge for invoking
`jsonSerialize()` without adding a second userland call mechanism inside the
runtime encoder.

## Prompt 17.2 JSON State

VM execution state now owns request-local JSON last-error code, seeds each
`BuiltinContext` from it, and copies the updated code back after builtin
dispatch. `json_decode` failures without `JSON_THROW_ON_ERROR` now return
`NULL`, matching PHP's decode failure shape, while still setting
`JSON_ERROR_SYNTAX`.

Focused coverage added:

- `builtin_context_persists_json_last_error_across_vm_builtin_calls`
- `tests/phpt/generated/json/json-last-error-state.phpt`

## Prompt 17.1 Harness

The selected manifest is intentionally narrow and covers:

- encode scalar/list/map/simple-object basics
- decode associative array and `stdClass` basics
- last-error code and message helpers
- common encode flags
- `JSON_THROW_ON_ERROR` failure routing

`JsonSerializable` remains deferred until the runtime builtin layer can call
back into normal VM method dispatch.
