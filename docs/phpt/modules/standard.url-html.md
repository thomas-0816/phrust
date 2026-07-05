# standard.url-html

- Priority: 16.8
- Selected manifest: `tests/phpt/manifests/modules/standard.url-html.selected.jsonl`
- Derived corpus baseline: 1 PASS, 0 SKIP, 63 FAIL, 5 BORK from 69 path-filtered candidates
- focused gate: 23 PASS, 0 FAIL, 0 BORK

## Scope

- URL encode/decode smoke coverage
- `http_build_query` array MVP coverage
- `http_build_query` null separator defaults, references, and RFC3986
  `encoding_type` coverage
- Basic `parse_url` component extraction and `PHP_URL_*` constant ordering
- Malformed `parse_str` key recovery and invalid percent preservation
- Default `htmlspecialchars` / `htmlentities` coverage

## Non-Scope

- Complete entity tables
- Non-default charsets and flags
- Object query encoding
- `parse_url` edge cases beyond the basic upstream corpus
- Remaining `parse_str` edge cases and URL/HTML upstream corpus

## Relevant PHPT Paths

- `ext/standard/tests/url/bug53248.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_with_null.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_with_references.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_variation2.phpt`
- `ext/standard/tests/http/http_build_query/bug26819.phpt`
- `ext/standard/tests/http/http_build_query/bug77608.phpt`
- `ext/standard/tests/http/http_build_query/gh12745.phpt`
- `ext/standard/tests/url/parse_url_basic_001.phpt`
- `ext/standard/tests/url/parse_url_basic_002.phpt`
- `ext/standard/tests/url/parse_url_basic_003.phpt`
- `ext/standard/tests/url/parse_url_basic_004.phpt`
- `ext/standard/tests/url/parse_url_basic_005.phpt`
- `ext/standard/tests/url/parse_url_basic_006.phpt`
- `ext/standard/tests/url/parse_url_basic_007.phpt`
- `ext/standard/tests/url/parse_url_basic_008.phpt`
- `ext/standard/tests/url/parse_url_basic_009.phpt`
- `ext/standard/tests/url/parse_url_basic_010.phpt`
- `ext/standard/tests/url/parse_url_basic_011.phpt`
- `ext/standard/tests/url/parse_url_relative_scheme.phpt`
- `ext/standard/tests/strings/parse_str_basic4.phpt`
- `tests/phpt/generated/standard.url-html/url-encode-decode-smoke.phpt`
- `tests/phpt/generated/standard.url-html/http-build-query-smoke.phpt`
- `tests/phpt/generated/standard.url-html/htmlspecialchars-htmlentities-smoke.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/strings.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `docs/stdlib-encoding-hash-url.md`
- `docs/stdlib-known-gaps.md`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.url-html`
- `nix develop -c just verify-stdlib`

## Evidence

- Added a dedicated selected manifest and generated smoke fixtures for the
  URL/HTML MVP.
- `PHP_QUERY_RFC1738` and `PHP_QUERY_RFC3986` are registered standard
  constants for query-string encoding mode selection.
- `http_build_query()` now keeps `null` separators on the PHP default `&`
  path and honors RFC3986 encoding for named `encoding_type` calls.
- `parse_url()` covers the basic upstream component extraction set, including
  partial numeric ports and `PHP_URL_*` iteration order through
  `get_defined_constants()`.
- `parse_str()` now matches PHP malformed bracket-key recovery for the selected
  upstream query fixture, including root dot/space normalization and invalid
  percent escape preservation.
- Latest focused target run: PASS, 23 selected PHPTs.

## Known Gaps

- Full entity-table, charset, flag, object-query, `parse_url` edge-case,
  remaining `parse_str`, and URL edge-case behavior remains outside the
  selected focused gate.
