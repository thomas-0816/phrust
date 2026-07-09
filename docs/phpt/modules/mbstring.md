# mbstring

- Strategy: bounded mbstring MVP with initial legacy encoding registry coverage
- Selected manifest: `tests/phpt/manifests/modules/mbstring.selected.jsonl`
- Selected gate: 116 PHPTs covering platform visibility, UTF-8 common
  functions, UTF-8/ASCII detection, request-local internal encoding, and narrow
  UTF-8 conversion, bounded UTF-8 position helpers, ISO-8859-1,
  Windows-1252, and Shift_JIS conversion/check/detect/length/substr/strpos,
  selected `mb_strrpos`/`mb_strripos` reverse-position behavior, selected
  `mb_substr_count` behavior, alias registry helpers,
  substitute-character state, and 111 target-green
  upstream rows
- Selected full gate with the default oracle: reference 116 SKIP, target 61 PASS
  and 55 SKIP from 116 selected fixtures.
- Selected target-only gate: 61 PASS, 55 SKIP, 0 FAIL, 0 BORK from 116 selected
  fixtures.
- Upstream target-only snapshot: 420 `ext/mbstring/tests` candidates measured
  as 56 PASS, 57 SKIP, 307 FAIL, 0 BORK. The selected target gate is expected
  to be 61 PASS and 55 SKIP after adding the five generated fixtures.

## Decision

Enable a deliberately narrow mbstring surface for Composer and framework probes:

- `mb_strlen`
- `mb_substr`
- `mb_strtolower`
- `mb_strtoupper`
- `mb_strpos`
- `mb_stripos`
- `mb_strrpos`
- `mb_strripos`
- `mb_substr_count`
- `mb_detect_encoding`
- `mb_check_encoding`
- `mb_internal_encoding`
- `mb_convert_encoding` for UTF-8, ASCII, 8bit, ISO-8859-1, Windows-1252,
  Shift_JIS, EUC-JP, and ISO-2022-JP
- `mb_list_encodings`
- `mb_encoding_aliases`
- `mb_substitute_character`
- `mb_strrpos` and `mb_strripos` implement selected reverse-search behavior,
  including empty-needle and negative-offset bounds from promoted upstream rows.
- `mb_strpos` and `mb_stripos` implement selected offset bounds, including
  PHP-compatible `ValueError` diagnostics for offsets outside the haystack.

The implementation uses the existing runtime plus `encoding_rs` for common
legacy encodings, with a custom ISO-8859-1 path so Latin-1 byte semantics do
not collapse into WHATWG Windows-1252 aliases. It does not introduce mbregex,
Oniguruma, locale data, or broad upstream `ext/mbstring` parity.

The default project oracle at `$REFERENCE_PHP` was built without mbstring. The
five generated mbstring fixtures therefore include `--SKIPIF--` guards for
`extension_loaded("mbstring")`: the default reference run skips them, while the
phrust target run still executes them because the bounded mbstring surface is
enabled there. A mbstring-enabled oracle is still required before promoting
additional upstream behavior that needs byte-for-byte reference output.

## Runtime Contract

- `extension_loaded("mbstring")` returns `true`.
- `function_exists()` returns `true` only for the selected MVP functions.
- Unsupported mbstring functions outside the selected surface remain absent
  rather than returning fake results.
- Supported encodings for selected APIs are `UTF-8`, `ASCII`, `8bit`,
  `ISO-8859-1`, `Windows-1252`, `SJIS`, `EUC-JP`, and `ISO-2022-JP` aliases
  accepted by the selected fixtures.
- `mb_internal_encoding()` is request-local runtime state with default `UTF-8`.
- `mb_substitute_character()` is request-local runtime state with default `63`
  and supports `none`, `long`, `entity`, and valid Unicode codepoints.
- `mb_list_encodings()` and `mb_encoding_aliases()` expose the encodings the
  selected runtime APIs actually support.
- Unsupported encodings return deterministic unsupported diagnostics or `false`
  at the selected API boundary.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/mbstring/platform-checks.phpt`
- `tests/phpt/generated/mbstring/utf8-common-functions.phpt`
- `tests/phpt/generated/mbstring/utf8-encoding-state.phpt`
- `tests/phpt/generated/mbstring/utf8-position-functions.phpt`
- `tests/phpt/generated/mbstring/legacy-encoding-registry.phpt`
- 111 target-green upstream rows from `ext/mbstring/tests`
- `tests/phpt/generated/wp.stdlib/text-encoding-basic.phpt` covers the
  WordPress stdlib text-encoding integration path.

These PHPTs keep the enabled surface explicit and reference-backed without
promoting the full upstream mbstring corpus.

## Optional PHPTs

Promote upstream tests only after matching their behavior with focused
reference-backed fixtures:

- UTF-8/common-function basics:
  - `ext/mbstring/tests/mb_strlen.phpt`
  - `ext/mbstring/tests/mb_strlen_basic.phpt`
  - `ext/mbstring/tests/mb_substr.phpt`
  - `ext/mbstring/tests/mb_substr_basic.phpt`
  - `ext/mbstring/tests/mb_strtolower_basic.phpt`
  - `ext/mbstring/tests/mb_strtoupper_basic.phpt`
  - `ext/mbstring/tests/mb_detect_encoding.phpt`
- Encoding error cases for the scoped functions:
  - `ext/mbstring/tests/mb_strlen_error2.phpt`
  - `ext/mbstring/tests/mb_substr_error2.phpt`
  - `ext/mbstring/tests/mb_strtolower_error2.phpt`
  - `ext/mbstring/tests/mb_strtoupper_error2.phpt`
  - `ext/mbstring/tests/mb_detect_encoding_empty_encoding_list.phpt`
- remaining `mb_substr_count` offset/length, `mb_convert_case`,
  `mb_strcut`, and full encoding-matrix rows

## Out-of-Scope PHPTs

Out of scope for this MVP:

- Exhaustive encoding verification/conversion suites such as
  `*_encoding.phpt`, `utf_encodings.phpt`, `sjis*_encoding.phpt`,
  `euc*_encoding.phpt`, `iso2022*_encoding.phpt`, `cp*_encoding.phpt`,
  `ucs2_encoding.phpt`, and `ucs4_encoding.phpt`.
- Conversion APIs outside the selected `mb_convert_encoding` scalar string
  matrix, including `mb_convert_kana`, `mb_convert_variables`,
  `mb_encode_mimeheader`, and `mb_decode_mimeheader`.
- mbstring regex and Oniguruma behavior, including `mb_regex_encoding*` and
  related regex/callback tests.
- HTTP/input/output encoding translation, `zend.multibyte`, mail/mime helpers,
  mobile carrier encodings, security regression tests for mbfl internals, and
  full bug-report regression coverage.
- `ext/exif` tests that require mbstring only as an implementation dependency.

## Unicode and Encoding Gaps

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `PHPT-MBSTRING-UNSUPPORTED-GRAPHEME-LENGTH` | PHP mbstring counts according to selected mbstring encoding tables and has broader invalid-sequence handling. | `mb_strlen` counts Rust `char` values after selected encoding decode and bytes for `8bit`; full grapheme and invalid-sequence behavior is not complete. | `tests/phpt/generated/mbstring/utf8-common-functions.phpt` | `php_runtime` mbstring implementation |
| `PHPT-MBSTRING-UNSUPPORTED-FULL-CASE-MAPPING` | PHP case mapping follows mbstring tables for selected encodings. | `mb_strtolower` and `mb_strtoupper` use Rust Unicode case mapping for selected UTF-8 examples only. | `tests/phpt/generated/mbstring/utf8-common-functions.phpt` | `php_runtime` mbstring implementation |
| `PHPT-MBSTRING-UNSUPPORTED-DETECT-ENCODING-MATRIX` | `mb_detect_encoding` depends on PHP's full supported encoding list, aliases, detection order, strict mode, and invalid-sequence handling. | Detection is limited to explicit selected encoding candidates and does not implement PHP's full default detection order. | `tests/phpt/generated/mbstring/utf8-encoding-state.phpt` | `php_runtime` mbstring implementation |
| `PHPT-MBSTRING-UNSUPPORTED-SUBSTR-COUNT-MATRIX` | PHP `mb_substr_count` applies the full mbstring encoding matrix and all selected invalid-sequence edge cases. | Selected UTF-8, 8bit, non-overlapping count, empty-needle, and unknown-encoding paths are implemented; the full encoding and invalid-sequence matrix remains incomplete. | `ext/mbstring/tests/mb_substr_count_basic.phpt` | `php_runtime` mbstring implementation |
| `PHPT-MBSTRING-UNSUPPORTED-LEGACY-ENCODINGS` | Shift-JIS, EUC-JP, ISO-2022-JP, Big5, GB18030, ISO-8859 variants, CP932, UTF-7, and related encodings require mbstring conversion tables. | Initial ISO-8859-1, Windows-1252, Shift_JIS, EUC-JP, and ISO-2022-JP conversion/check support is enabled for selected APIs; the full PHP matrix and substitution behavior are not complete. | `tests/phpt/generated/mbstring/legacy-encoding-registry.phpt` | `php_runtime` mbstring implementation |
| `PHPT-MBSTRING-UNSUPPORTED-MBREGEX` | PHP mbstring regex uses Oniguruma-backed behavior. | No mbregex APIs are exposed. | none selected | future mbregex layer |

## Target Gates

- `nix develop -c cargo test -p php_runtime mbstring -- --nocapture`
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-mbstring-selected-expanded nix develop -c just phpt-dev-module MODULE=mbstring`
- `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_MANIFEST=/private/tmp/phrust-mbstring-full-manifest.jsonl PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-mbstring-full-target-strpos nix develop -c just phpt-module-target MODULE=mbstring` (expected non-zero while 307 full-corpus target failures remain)
- `nix develop -c cargo test -p php_std mbstring`
