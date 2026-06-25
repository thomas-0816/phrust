# json

- Priority: 17
- Selected manifest: `tests/phpt/manifests/modules/json.selected.jsonl`
- Current counts: 10 PASS, 1 SKIP, 77 FAIL, 0 BORK from 88 corpus candidates

## Scope

- json_encode
- json_decode
- json last-error state

## Non-Scope

- full JsonSerializable without object model readiness

## Relevant PHPT Paths

- `ext/json/tests/unsupported_type_error.phpt`
- `ext/json/tests/serialize.phpt`
- `ext/json/tests/pass003.phpt`
- `ext/json/tests/pass001.phpt`
- `ext/json/tests/pass001.1_64bit.phpt`
- `ext/json/tests/pass001.1.phpt`
- `ext/json/tests/json_validate_005.phpt`
- `ext/json/tests/json_validate_004.phpt`
- `ext/json/tests/json_validate_003.phpt`
- `ext/json/tests/json_validate_002.phpt`
- `ext/json/tests/json_last_error_msg_error.phpt`
- `ext/json/tests/json_last_error_error.phpt`
- `ext/json/tests/json_exceptions_error_clearing.phpt`
- `ext/json/tests/json_encode_unescaped_slashes.phpt`
- `ext/json/tests/json_encode_u2028_u2029.phpt`
- `ext/json/tests/json_encode_recursion_06.phpt`
- `ext/json/tests/json_encode_recursion_05.phpt`
- `ext/json/tests/json_encode_recursion_04.phpt`
- `ext/json/tests/json_encode_recursion_03.phpt`
- `ext/json/tests/json_encode_recursion_02.phpt`
- `ext/json/tests/json_encode_recursion_01.phpt`
- `ext/json/tests/json_encode_pretty_print2.phpt`
- `ext/json/tests/json_encode_pretty_print.phpt`
- `ext/json/tests/json_encode_numeric.phpt`
- `ext/json/tests/json_encode_invalid_utf8.phpt`
- `ext/json/tests/json_encode_exceptions.phpt`
- `ext/json/tests/json_encode_basic_utf8.phpt`
- `ext/json/tests/json_encode_basic.phpt`
- `ext/json/tests/json_decode_invalid_utf8.phpt`
- `ext/json/tests/json_decode_exceptions.phpt`
- `ext/json/tests/json_decode_error.phpt`
- `ext/json/tests/json_decode_basic.phpt`
- `ext/json/tests/inf_nan_error.phpt`
- `ext/json/tests/gh15168.phpt`
- `ext/json/tests/fail001.phpt`
- `ext/json/tests/bug81532.phpt`
- `ext/json/tests/bug77843.phpt`
- `ext/json/tests/bug73254.phpt`
- `ext/json/tests/bug73113.phpt`
- `ext/json/tests/bug72787.phpt`

## Relevant php-src Source Areas

- `ext/json/tests/`

## Target Gates

- `nix develop -c just phpt-module MODULE=json`

## Known Gaps

- `runtime-error-or-diagnostic`: 44
- `runtime-output-mismatch`: 27
- `runtime-unsupported-feature`: 8

## Next Step

Close request-local JSON error state and common flags.
