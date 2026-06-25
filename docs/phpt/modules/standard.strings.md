# standard.strings

- Priority: 13
- Selected manifest: `tests/phpt/manifests/modules/standard.strings.selected.jsonl`
- Current counts: 83 PASS, 0 SKIP, 619 FAIL, 0 BORK from 727 corpus candidates

## Scope

- ext/standard string builtins

## Non-Scope

- frontend literal decoding

## Relevant PHPT Paths

- `ext/standard/tests/strings/wordwrap_variation5.phpt`
- `ext/standard/tests/strings/wordwrap_memory_limit_32bit.phpt`
- `ext/standard/tests/strings/wordwrap_memory_limit.phpt`
- `ext/standard/tests/strings/wordwrap_error.phpt`
- `ext/standard/tests/strings/wordwrap_basic.phpt`
- `ext/standard/tests/strings/wordwrap.phpt`
- `ext/standard/tests/strings/vsprintf_basic7.phpt`
- `ext/standard/tests/strings/vsprintf_basic6.phpt`
- `ext/standard/tests/strings/vsprintf_basic4.phpt`
- `ext/standard/tests/strings/vprintf_variation8.phpt`
- `ext/standard/tests/strings/vprintf_variation7.phpt`
- `ext/standard/tests/strings/vprintf_variation6.phpt`
- `ext/standard/tests/strings/vprintf_variation5.phpt`
- `ext/standard/tests/strings/vprintf_variation4_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation4.phpt`
- `ext/standard/tests/strings/vprintf_variation3.phpt`
- `ext/standard/tests/strings/vprintf_variation2.phpt`
- `ext/standard/tests/strings/vprintf_variation19_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation19.phpt`
- `ext/standard/tests/strings/vprintf_variation18.phpt`
- `ext/standard/tests/strings/vprintf_variation17.phpt`
- `ext/standard/tests/strings/vprintf_variation16_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation16.phpt`
- `ext/standard/tests/strings/vprintf_variation15_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation15.phpt`
- `ext/standard/tests/strings/vprintf_variation14_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation14.phpt`
- `ext/standard/tests/strings/vprintf_variation13_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation13.phpt`
- `ext/standard/tests/strings/vprintf_variation12_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation12.phpt`
- `ext/standard/tests/strings/vprintf_variation11_64bit.phpt`
- `ext/standard/tests/strings/vprintf_variation11.phpt`
- `ext/standard/tests/strings/vprintf_basic7.phpt`
- `ext/standard/tests/strings/vprintf_basic6.phpt`
- `ext/standard/tests/strings/vprintf_basic4.phpt`
- `ext/standard/tests/strings/vfprintf_variation1.phpt`
- `ext/standard/tests/strings/vfprintf_error4.phpt`
- `ext/standard/tests/strings/vfprintf_error3.phpt`
- `ext/standard/tests/strings/vfprintf_error1.phpt`

## Relevant php-src Source Areas

- `ext/standard/tests/strings/`
- `tests/strings/`

## Target Gates

- `nix develop -c just phpt-module MODULE=standard.strings`

## Known Gaps

- `runtime-error-or-diagnostic`: 378
- `runtime-unsupported-feature`: 148
- `runtime-output-mismatch`: 86
- `frontend-parse-or-compile`: 7
- `runtime-timeout`: 2

## Next Step

Close common binary-safe string functions against Reference PHP.
