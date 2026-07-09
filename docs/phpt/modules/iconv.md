# iconv

- Strategy: bounded encoding conversion MVP
- Selected manifest: `tests/phpt/manifests/modules/iconv.selected.jsonl`
- Current selected target snapshot: 34 PASS, 15 SKIP, 0 FAIL, 0 BORK from 49
  selected fixtures
- Current selected reference snapshot: 49 SKIP, 0 FAIL, 0 BORK because the
  pinned `php-8.5.7` CLI build does not load iconv
- Full upstream target sweep: 35 PASS, 16 SKIP, 27 FAIL from 78
  `ext/iconv/tests/*.phpt` fixtures

## Selected PHPT Fixtures

- `tests/phpt/generated/iconv/basic.phpt`
- `tests/phpt/generated/iconv/aliases.phpt`
- `tests/phpt/generated/iconv/conversion-options.phpt`
- `tests/phpt/generated/iconv/substr-out-of-bounds.phpt`
- `tests/phpt/generated/iconv/strrpos.phpt`
- `tests/phpt/generated/iconv/mime-basic.phpt`
- `tests/phpt/generated/iconv/mime-decode-headers-basic.phpt`
- `tests/phpt/generated/iconv/mime-decode-folding.phpt`
- 41 target-green upstream iconv fixtures from `ext/iconv/tests`

## Implemented Surface

The runtime exposes `iconv`, `iconv_strlen`, `iconv_substr`, `iconv_strpos`,
`iconv_strrpos`, `iconv_mime_encode`, `iconv_mime_decode`,
`iconv_mime_decode_headers`, `iconv_get_encoding`, and `iconv_set_encoding`.

Supported encodings are `UTF-8`, `ASCII`, `ISO-8859-1`, `ISO-8859-2`,
`Windows-1252`, `SJIS`, `EUC-JP`, and `ISO-2022-JP` aliases. The selected
alias slice covers common ASCII names such as `ANSI_X3.4-1968` and `CP367`,
plus Latin-1 names such as `CP819` and `ISO-IR-100`. Encoding state is
request-local and defaults to `UTF-8`. The promoted INI rows cover the
legacy `input_encoding`, `internal_encoding`, and `output_encoding` aliases,
their `iconv.*` counterparts, and `default_charset` fallback behavior.

`iconv_substr` clamps empty computed ranges before slicing so out-of-bounds
requests return an empty string instead of raising an internal panic.
`iconv` recognizes selected `//IGNORE` and `//TRANSLIT` target-encoding
options for currently supported encodings.
`iconv_strrpos` is covered for selected ASCII/UTF-8 and EUC-JP reverse-search
behavior.
Unsupported explicit charset arguments now follow PHP's warning-and-`false`
shape for the promoted `iconv_strlen`, `iconv_substr`, `iconv_strpos`, and
`iconv_strrpos` rows. Explicit encoding names longer than 64 bytes also warn
and return `false` for the promoted `iconv`, `iconv_set_encoding`,
`iconv_mime_encode`, `iconv_mime_decode`, `iconv_mime_decode_headers`, and
string-position rows. Optional iconv string-position defaults consult the
request-local `default_charset` when iconv's internal encoding remains at the
default, matching the promoted invalid-default-charset row.
`iconv_mime_encode` and `iconv_mime_decode` cover selected UTF-8 B/Q encoded
word behavior, ISO-8859 charset suffixes, and adjacent encoded-word whitespace
folding. They expose `ICONV_MIME_DECODE_STRICT` and
`ICONV_MIME_DECODE_CONTINUE_ON_ERROR`, route malformed MIME warnings through
registered error handlers, and cover the promoted upstream
`iconv_mime_decode.phpt` fixture.
`iconv_mime_decode_headers` covers selected encoded-word values, folded line
joining, duplicate header names as packed arrays, and the promoted upstream
`iconv_mime_decode_headers.phpt` fixture.

## Gaps

The full iconv encoding database, broad transliteration/ignore-mode parity,
remaining MIME header edge cases, illegal-input notice parity, and stateful
ISO-2022-JP position parity remain out of scope. The 27 remaining upstream
target failures cover stream filters, output-buffer callbacks, XML/DOM
dependencies, full charset behavior, memory-limit probes, and MIME/header cases
outside the selected promoted fixtures.

## Target Gates

- `nix develop -c cargo test -p php_runtime iconv --no-fail-fast`
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-iconv-selected-ini-promoted nix develop -c just phpt-dev-module MODULE=iconv`
- `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_MANIFEST=/private/tmp/phrust-iconv-full-manifest.jsonl PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-iconv-full-target-ini-promoted nix develop -c just phpt-module-target MODULE=iconv` (expected non-zero while 27 full-corpus target failures remain)
