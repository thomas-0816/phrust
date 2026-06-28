# mbstring

- Strategy: minimal stubs
- Selected manifest: `tests/phpt/manifests/modules/mbstring.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility and guarded
  common-function probes
- Corpus snapshot: 420 `mbstring`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed baseline counts are
  3 PASS, 36 SKIP, 360 FAIL, 21 BORK, and 414 known non-green outcomes.

## Decision

Use explicit stubs now, not a real mbstring implementation.

The current dependency set has no approved Unicode or text-encoding library
beyond Rust standard-library primitives and transitive `unicode-ident`.
Implementing `mb_strlen`, `mb_substr`, `mb_strtolower`, `mb_strtoupper`, or
`mb_detect_encoding` with ad hoc UTF-8-only behavior would make Composer and
framework probes believe mbstring exists while leaving large PHP-visible
Unicode and legacy-encoding gaps. That is worse than a clear negative platform
check.

## Runtime Contract

- `extension_loaded("mbstring")` returns `false`.
- `function_exists()` for the selected mbstring functions returns `false` while
  the mbstring extension remains disabled in the standard-library registry.
- Direct internal calls to the registered runtime stubs fail with
  `E_PHP_RUNTIME_UNSUPPORTED_MBSTRING`; they do not return fake string lengths,
  substrings, case conversions, or encoding names.
- No new Rust Unicode or encoding dependency is introduced by this slice.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/mbstring/platform-checks.phpt`
- `tests/phpt/generated/mbstring/guarded-common-functions.phpt`
- `tests/phpt/generated/mbstring/composer-fallback.phpt`

These PHPTs make the platform decision visible: mbstring is unavailable, and
common mbstring helpers must be guarded by `extension_loaded()` or
`function_exists()` checks.

## Optional PHPTs

Optional only after selecting a real implementation strategy:

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

These become promotable only when mbstring is intentionally enabled and the
Unicode parity gaps below have concrete tests.

## Out-of-Scope PHPTs

Out of scope for the stub strategy:

- Exhaustive encoding verification/conversion suites such as
  `*_encoding.phpt`, `utf_encodings.phpt`, `sjis*_encoding.phpt`,
  `euc*_encoding.phpt`, `iso2022*_encoding.phpt`, `cp*_encoding.phpt`,
  `ucs2_encoding.phpt`, and `ucs4_encoding.phpt`.
- Conversion APIs outside the selected Composer/platform surface, including
  `mb_convert_encoding*`, `mb_convert_kana`, `mb_convert_variables`,
  `mb_encode_mimeheader`, and `mb_decode_mimeheader`.
- mbstring regex and Oniguruma behavior, including `mb_regex_encoding*` and
  related regex/callback tests.
- HTTP/input/output encoding translation, `zend.multibyte`, mail/mime helpers,
  mobile carrier encodings, security regression tests for mbfl internals, and
  full bug-report regression coverage.
- `ext/exif` tests that require mbstring only as an implementation dependency.

## Unicode Parity Gaps

If this moves from stubs to implementation, the first implementation must
document and test these gaps before enabling `extension_loaded("mbstring")`:

- PHP counts characters by the selected mbstring encoding, not Rust bytes or
  necessarily Unicode scalar values.
- PHP's case mapping follows mbstring tables for the selected encoding; Rust
  `char::to_lowercase` and `char::to_uppercase` are not a complete parity
  model.
- `mb_detect_encoding` depends on PHP's supported encoding list, detection
  order, strict mode, aliases, and invalid-sequence handling.
- Legacy encodings such as Shift-JIS, EUC-JP, ISO-2022-JP, Big5, GB18030,
  ISO-8859 variants, CP932, and UTF-7 require an approved encoding library or a
  deliberately bounded table strategy.

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mbstring`
- `nix develop -c just verify-stdlib`
