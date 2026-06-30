# Wave 5A Stdlib Core Diagnostics Serialization Current

## Scope

- Branch: `wave5a-stdlib-core-diagnostics-serialization`
- Owned stream: Wave 5A stdlib core diagnostics and serialization
- Focused promotion slice: `ext/json/tests/001.phpt` through
  `ext/json/tests/009.phpt`

## Baseline

- Initial selected module gates were green:
  - `json`: 15 PASS
  - `pcre`: 11 PASS
  - `date`: 9 PASS
  - `standard.serialization`: 23 PASS
  - `standard.variables`: 33 PASS
- Early `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just verify-stdlib`
  passed before implementation.

## JSON Probe

- Candidate manifest: `/tmp/wave5a-json-candidates.jsonl`
- Reference probe: 9 PASS, 0 non-green
- Initial target probe: 4 PASS, 5 FAIL
- Final target probe: 9 PASS, 0 non-green

## Promoted Fixtures

- `ext/json/tests/001.phpt`
- `ext/json/tests/002.phpt`
- `ext/json/tests/003.phpt`
- `ext/json/tests/004.phpt`
- `ext/json/tests/005.phpt`
- `ext/json/tests/006.phpt`
- `ext/json/tests/007.phpt`
- `ext/json/tests/008.phpt`
- `ext/json/tests/009.phpt`

## Implemented Behavior

- `JSON_FORCE_OBJECT` maps selected arrays to JSON objects.
- Default `json_encode` output escapes non-ASCII as PHP does unless
  `JSON_UNESCAPED_UNICODE` is set.
- `json_decode` enforces selected depth failures and maps selected malformed
  JSON cases to state-mismatch, control-character, or syntax errors.
- `JSON_BIGINT_AS_STRING` preserves selected oversized integer literals.
- `json_encode` detects selected recursive array/object graphs and supports
  `JSON_PARTIAL_OUTPUT_ON_ERROR`.

## Remaining Non-Scope

- Full `JsonSerializable` dispatch remains
  `STDLIB-GAP-JSONSERIALIZABLE-DISPATCH` until runtime builtins have a clean VM
  bridge for userland method calls.
- Invalid UTF-8 recovery modes and the full upstream `ext/json` corpus remain
  outside this selected promotion.

## Validation

- `nix develop -c cargo test -p php_runtime json`: PASS
- Target candidate probe for `ext/json/tests/001.phpt` through `009.phpt`: 9
  PASS
- Reference candidate probe for `ext/json/tests/001.phpt` through `009.phpt`: 9
  PASS
- `REFERENCE_PHP=... PHP_SRC_DIR=... PHPT_REUSE_LAST=0
  PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module
  MODULE=json`: 24 reference PASS, 24 target PASS
- Required selected module gates remained green:
  - `pcre`: 11 reference PASS, 11 target PASS
  - `date`: 9 reference PASS, 9 target PASS
  - `standard.serialization`: 23 reference PASS, 23 target PASS
  - `standard.variables`: 33 reference PASS, 33 target PASS
- `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just verify-stdlib`: PASS
- `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just verify-runtime`: PASS
- `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just verify-phpt`: PASS
