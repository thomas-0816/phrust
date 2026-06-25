# standard.variables

- Priority: 15
- Selected manifest: `tests/phpt/manifests/modules/standard.variables.selected.jsonl`
- Current counts: 23 PASS, 74 SKIP, 348 FAIL, 0 BORK from 446 corpus candidates

## Scope

- variable inspection and conversion builtins

## Non-Scope

- general VM symbol-table redesign

## Relevant PHPT Paths

- `ext/standard/tests/versioning/version_compare_op_abbrev.phpt`
- `ext/standard/tests/versioning/version_compare_invalid_operator.phpt`
- `ext/standard/tests/versioning/version_compare.phpt`
- `ext/standard/tests/versioning/phpversion.phpt`
- `ext/standard/tests/versioning/php_sapi_name_variation001.phpt`
- `ext/standard/tests/versioning/php_sapi_name.phpt`
- `ext/standard/tests/url/parse_url_unterminated.phpt`
- `ext/standard/tests/url/parse_url_relative_scheme.phpt`
- `ext/standard/tests/url/parse_url_error_002.phpt`
- `ext/standard/tests/url/parse_url_basic_011.phpt`
- `ext/standard/tests/url/parse_url_basic_010.phpt`
- `ext/standard/tests/url/parse_url_basic_009.phpt`
- `ext/standard/tests/url/parse_url_basic_008.phpt`
- `ext/standard/tests/url/parse_url_basic_007.phpt`
- `ext/standard/tests/url/parse_url_basic_006.phpt`
- `ext/standard/tests/url/parse_url_basic_005.phpt`
- `ext/standard/tests/url/parse_url_basic_004.phpt`
- `ext/standard/tests/url/parse_url_basic_003.phpt`
- `ext/standard/tests/url/parse_url_basic_002.phpt`
- `ext/standard/tests/url/parse_url_basic_001.phpt`
- `ext/standard/tests/url/get_headers_error_003.phpt`
- `ext/standard/tests/url/bug74780.phpt`
- `ext/standard/tests/url/bug73192.phpt`
- `ext/standard/tests/url/bug69976.phpt`
- `ext/standard/tests/url/bug68917.phpt`
- `ext/standard/tests/url/bug63162.phpt`
- `ext/standard/tests/url/bug55399.phpt`
- `ext/standard/tests/url/bug55273.phpt`
- `ext/standard/tests/url/bug54180.phpt`
- `ext/standard/tests/url/bug52327.phpt`
- `ext/standard/tests/url/bug47174.phpt`
- `ext/standard/tests/url/base64_loop_001.phpt`
- `ext/standard/tests/url/base64_encode_basic_002.phpt`
- `ext/standard/tests/url/base64_encode_basic_001.phpt`
- `ext/standard/tests/url/base64_decode_basic_003.phpt`
- `ext/standard/tests/url/base64_decode_basic_002.phpt`
- `ext/standard/tests/url/base64_decode_basic_001.phpt`
- `ext/standard/tests/time/strptime_parts.phpt`
- `ext/standard/tests/time/strptime_basic.phpt`
- `ext/standard/tests/time/idate_iso.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/general_functions/`
- `ext/standard/tests/array/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.variables`

## Known Gaps

- `runtime-error-or-diagnostic`: 226
- `runtime-unsupported-feature`: 153
- `runtime-output-mismatch`: 42
- `frontend-parse-or-compile`: 14

## Next Step

Stabilize var_dump/print_r/serialization-adjacent value rendering.
