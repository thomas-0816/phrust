# iconv

- Strategy: bounded encoding conversion MVP
- Selected manifest: `tests/phpt/manifests/modules/iconv.selected.jsonl`
- Current selected target snapshot: 9 PASS, 15 SKIP, 0 FAIL, 0 BORK from 24
  selected fixtures
- Current selected reference snapshot: 24 SKIP, 0 FAIL, 0 BORK because the
  pinned `php-8.5.7` CLI build does not load iconv
- Full upstream target sweep: 6 PASS, 15 SKIP, 55 FAIL from 76
  `ext/iconv/tests/*.phpt` fixtures

## Selected PHPT Fixtures

- `tests/phpt/generated/iconv/basic.phpt`
- `tests/phpt/generated/iconv/aliases.phpt`
- `tests/phpt/generated/iconv/substr-out-of-bounds.phpt`
- 21 target-green upstream iconv fixtures from `ext/iconv/tests`

## Implemented Surface

The runtime exposes `iconv`, `iconv_strlen`, `iconv_substr`, `iconv_strpos`,
`iconv_get_encoding`, and `iconv_set_encoding`.

Supported encodings are `UTF-8`, `ASCII`, and `ISO-8859-1` aliases. The
selected alias slice covers common ASCII names such as `ANSI_X3.4-1968` and
`CP367`, plus Latin-1 names such as `CP819` and `ISO-IR-100`. Encoding state is
request-local and defaults to `UTF-8`.

`iconv_substr` clamps empty computed ranges before slicing so out-of-bounds
requests return an empty string instead of raising an internal panic.

## Gaps

The full iconv encoding database, transliteration, ignore-mode parity, and
legacy multibyte encodings remain out of scope. The 55 remaining upstream target
failures cover MIME header helpers, `iconv_strrpos`, stream filters, exact
warning text, long charset-name guards, and broader charset behavior.

## Target Gates

- `nix develop -c cargo test -p php_runtime iconv --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 nix develop -c just phpt-dev-module MODULE=iconv`
