# json

- Priority: 17.6 closed
- Selected manifest: `tests/phpt/manifests/modules/json.selected.jsonl`
- Prompt 17 close gate: 10 PASS, 0 SKIP, 0 FAIL, 0 BORK from 10 selected fixtures

## Scope

- `json_encode` scalar, list, object-map, simple object, and common flag basics
- `json_decode` associative-array and `stdClass` basics
- `json_last_error` and `json_last_error_msg`
- `JSON_THROW_ON_ERROR` failure routing

## Non-Scope

- Full upstream `ext/json` corpus
- `JsonSerializable` userland dispatch until JSON encoding has a clean VM
  method-call bridge
- Bigint preservation, invalid UTF-8 modes, recursion diagnostics, and complete
  JSON flag parity
- `json_validate`

## Selected PHPT Fixtures

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

## Relevant Source Areas

- `crates/php_runtime/src/builtins/context.rs`
- `crates/php_runtime/src/builtins/modules/json.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=json`
- `nix develop -c just verify-phpt`

## Prompt 17.1 Evidence

- Replaced the broad 88-test selected manifest with a focused 10-test harness.
- Added generated oracle fixtures for encode basics, encode common flags, decode
  basics, last-error state transitions, and `JSON_THROW_ON_ERROR`.
- Latest focused target run: 10 PASS, 0 FAIL.
- Latest focused reference run: 10 PASS.

## Known Gaps

- Request-local JSON last-error state now persists across VM builtin calls.
- `json_encode` now matches the selected scalar/list/map/simple-object, common
  flag, slash escaping, pretty-print, and insertion-order PHPTs.
- `JSON_THROW_ON_ERROR` decode failures now route to catchable `JsonException`
  through the existing VM throwable path.
- `JsonSerializable` is deliberately left as `STDLIB-GAP-JSONSERIALIZABLE-DISPATCH`:
  `json_encode` is a runtime builtin and there is not yet a clean VM method-call
  bridge for invoking userland `jsonSerialize()`.

## Next Step

Prompt 17 is closed for the selected JSON module. Keep
`STDLIB-GAP-JSONSERIALIZABLE-DISPATCH` as the next semantic expansion point once
runtime builtins have a clean bridge into VM userland method dispatch.
