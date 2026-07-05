# filter

- Strategy: validation and sanitization MVP
- Selected manifest: `tests/phpt/manifests/modules/filter.selected.jsonl`
- Selected close gate: 39 PASS, 0 SKIP, 0 FAIL, 0 BORK from 39 selected fixtures
- Upstream corpus snapshot before the selected gate: 36 PASS, 3 XFAIL, 75 FAIL,
  0 BORK from 114 corpus candidates
- Selected fixtures:
  - `tests/phpt/generated/filter/basic.phpt`
  - `tests/phpt/generated/filter/arrays.phpt`
  - `tests/phpt/generated/filter/options-callback.phpt`
  - 36 target-green upstream rows from `ext/filter/tests`

## Implemented Surface

`filter_var` covers `FILTER_DEFAULT`/`FILTER_UNSAFE_RAW`, `FILTER_VALIDATE_EMAIL`,
`FILTER_VALIDATE_URL`, `FILTER_VALIDATE_INT`, `FILTER_VALIDATE_FLOAT`,
`FILTER_VALIDATE_BOOLEAN`, `FILTER_SANITIZE_EMAIL`, and
`FILTER_SANITIZE_URL`. It also handles `FILTER_REQUIRE_ARRAY`,
`FILTER_FORCE_ARRAY`, `FILTER_REQUIRE_SCALAR`, and
`FILTER_SANITIZE_NUMBER_FLOAT` with the common fraction, thousand, and
scientific flags.

The selected slice also includes `filter_has_var`, `filter_input_array`,
`filter_var_array`, `filter_list`, and `filter_id` for common request and
metadata paths. Array filter specs support integer filter IDs plus nested
`filter`, `flags`, and `options` entries. `FILTER_VALIDATE_INT` and
`FILTER_VALIDATE_FLOAT` honor `min_range` and `max_range` option arrays.
`FILTER_VALIDATE_INT` also follows PHP's accepted hexadecimal and octal
forms, including unsigned-prefixed overflow wrapping for the
`FILTER_FLAG_ALLOW_HEX` and `FILTER_FLAG_ALLOW_OCTAL` paths. The standard
registry exposes the php-src filter flag constant surface used by upstream
PHPT option arrays.

`FILTER_VALIDATE_EMAIL` enforces PHP's local-part, label, and total length
limits for the promoted fixtures. `FILTER_VALIDATE_BOOLEAN` returns the normal
failure value when object string conversion is invalid.

`FILTER_CALLBACK` is exposed in `filter_list`, `filter_id`, and the standard
constant registry. The runtime executes callback filters for registered builtin
string callables such as `strtoupper`.

Unsupported filter identifiers return the normal filter failure value
(`false` or `null` with `FILTER_NULL_ON_FAILURE`) instead of accepting unknown
behavior.

The CLI startup path emits the upstream `filter.default` deprecation diagnostic
when PHPT-style startup error display is enabled.

## Gaps

The full PHP filter option matrix, remaining exact filter flag behavior,
request input edge cases, VM-dispatched user function and closure callbacks,
throw-on-failure mode, remaining exact warning/deprecation text, and
locale-specific numeric parsing remain out of scope.

The full upstream target sweep measured 36 PASS, 3 XFAIL, and 75 FAIL from 114
`ext/filter/tests` rows. The remaining unpromoted rows are dominated by
stricter PHP URL/email/IP quirks, remaining filter flag behavior, callback
dispatch, request/superglobal edge cases, array-to-string conversion behavior,
and exact warning/deprecation output.

## Target Gates

- `nix develop -c cargo test -p php_runtime filter`
- `nix develop -c cargo test -p php_std filter`
- `nix develop -c cargo test -p php_server server_filter_input_array_reads_query_snapshot`
- `nix develop -c just phpt-dev-module MODULE=filter`
