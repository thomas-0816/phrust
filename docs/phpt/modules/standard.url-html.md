# standard.url-html

- Priority: 16.8
- Selected manifest: `tests/phpt/manifests/modules/standard.url-html.selected.jsonl`
- Derived corpus baseline: 1 PASS, 0 SKIP, 68 FAIL, 5 BORK from 81 path-filtered candidates
- focused gate: 54 PASS, 0 FAIL, 0 BORK

## Scope

- URL encode/decode smoke coverage
- `http_build_query` array and visible-object property coverage
- `http_build_query` null/output separator defaults, references, resources,
  recursive object suppression, scoped property visibility, and RFC3986
  `encoding_type` coverage
- Basic `parse_url` component extraction, negative component fallback, invalid
  component diagnostics, and `PHP_URL_*` constant ordering
- `parse_str` basics, custom `arg_separator.input`, malformed key recovery, and
  invalid percent preservation
- Default `htmlspecialchars` / `htmlentities` coverage, quote-flag-sensitive
  `htmlspecialchars_decode`, numeric-basic `htmlspecialchars_decode`, and
  selected `html_entity_decode` document-type and numeric entity filtering
- `get_html_translation_table` coverage for special-character quote modes,
  XML 1.0 basic entities, and HTML5/SJIS basic entities
- `getenv()` / `putenv()` request-environment lookup, mutation, unset,
  empty-value, no-argument array snapshot, local-only lookup, UTF-8 value, and
  invalid-assignment `ValueError` coverage

## Non-Scope

- Complete HTML4/HTML5/XHTML entity tables
- Non-default charsets beyond selected UTF-8 and SJIS basic-entity coverage
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
- `ext/standard/tests/http/http_build_query/http_build_query_variation3.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_with_resource.phpt`
- `ext/standard/tests/http/http_build_query/bug26817.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_basic.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_empty.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_just_stringable.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_key_val_stringable.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_nested.phpt`
- `ext/standard/tests/http/http_build_query/http_build_query_object_recursif.phpt`
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
- `ext/standard/tests/url/parse_url_error_002.phpt`
- `ext/standard/tests/url/parse_url_relative_scheme.phpt`
- `ext/standard/tests/strings/parse_str_basic1.phpt`
- `ext/standard/tests/strings/parse_str_basic2.phpt`
- `ext/standard/tests/strings/parse_str_basic4.phpt`
- `ext/standard/tests/strings/parse_str_memory_error.phpt`
- `ext/standard/tests/strings/htmlspecialchars_basic.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_basic.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_variation3.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_variation4.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_variation5.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_variation6.phpt`
- `ext/standard/tests/strings/htmlspecialchars_decode_variation7.phpt`
- `ext/standard/tests/strings/htmlspecialchars.phpt`
- `ext/standard/tests/strings/html_entity_decode2.phpt`
- `ext/standard/tests/strings/html_entity_decode3.phpt`
- `ext/standard/tests/strings/get_html_translation_table_basic3.phpt`
- `ext/standard/tests/strings/get_html_translation_table_basic8.phpt`
- `ext/standard/tests/strings/get_html_translation_table_basic9.phpt`
- `ext/standard/tests/general_functions/getenv.phpt`
- `ext/standard/tests/general_functions/putenv.phpt`
- `ext/standard/tests/general_functions/bug50690.phpt`
- `ext/standard/tests/general_functions/bug79254.phpt`
- `ext/standard/tests/general_functions/putenv_bug75574_utf8.phpt`
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
  path, reads `arg_separator.output` through the shared INI registry, omits
  resource leaves, and honors RFC3986 encoding for named `encoding_type` calls.
- `http_build_query()` now accepts object input, emits caller-visible
  properties with PHP visibility rules at the top level, handles nested public
  object properties, treats stringable objects as property containers, preserves
  empty-object output, and suppresses recursive object cycles.
- `parse_url()` covers the selected upstream component extraction set,
  including partial numeric ports, negative component fallback to the full
  array, invalid positive component `ValueError` messages, and `PHP_URL_*`
  iteration order through `get_defined_constants()`.
- `parse_str()` now matches PHP basic result-array population, custom
  `arg_separator.input` characters, malformed bracket-key recovery for the
  selected upstream query fixture, root dot/space normalization, invalid
  percent escape preservation, and the selected memory-safety regression.
- `htmlspecialchars_decode()` now honors `ENT_COMPAT`, `ENT_NOQUOTES`, and
  `ENT_QUOTES` quote decoding, binary/heredoc inputs, document-type-sensitive
  `&apos;`, and numeric basic-character entities for the selected upstream
  basic and variation PHPTs.
- `html_entity_decode()` now honors quote and document-type flags for `&apos;`
  and decodes numeric entities only when the selected HTML 4.01, XHTML, HTML5,
  XML 1.0, and default UTF-8 contracts allow the code point.
- `get_html_translation_table()` now returns PHP arrays for
  `HTML_SPECIALCHARS` quote modes plus XML 1.0 and HTML5/SJIS basic
  `HTML_ENTITIES` tables.
- `getenv()` and `putenv()` now cover request-local environment mutation,
  array snapshots after mutation, empty-string values, unsetting, local-only
  lookup, UTF-8 values, and catchable `ValueError` invalid assignment syntax.
- Latest focused target run: PASS, 54 selected PHPTs.

## Known Gaps

- Full HTML4/HTML5/XHTML entity-table, charset, `parse_url` edge-case,
  remaining `parse_str`, including the separate startup `filter.default`
  deprecation-output mismatch in `parse_str_basic3.phpt`, and URL edge-case
  behavior remains outside the selected focused gate.
