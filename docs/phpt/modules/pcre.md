# pcre

- Priority: 18
- Selected manifest: `tests/phpt/manifests/modules/pcre.selected.jsonl`
- Current counts: 41 PASS, 5 SKIP, 117 FAIL, 0 BORK from 165 corpus candidates

## Scope

- preg_* builtins backed by PCRE2

## Non-Scope

- PCRE JIT/callout parity

## Relevant PHPT Paths

- `ext/pcre/tests/ungreedy.phpt`
- `ext/pcre/tests/study.phpt`
- `ext/pcre/tests/split2.phpt`
- `ext/pcre/tests/split.phpt`
- `ext/pcre/tests/request47456.phpt`
- `ext/pcre/tests/recursion_limit.phpt`
- `ext/pcre/tests/preg_split_error1.phpt`
- `ext/pcre/tests/preg_replace_variation1.phpt`
- `ext/pcre/tests/preg_replace_error2.phpt`
- `ext/pcre/tests/preg_replace_error1.phpt`
- `ext/pcre/tests/preg_replace_edit_basic.phpt`
- `ext/pcre/tests/preg_replace_callback_trampoline.phpt`
- `ext/pcre/tests/preg_replace_callback_flags.phpt`
- `ext/pcre/tests/preg_replace_callback_fatal_error_leak.phpt`
- `ext/pcre/tests/preg_replace_callback_error1.phpt`
- `ext/pcre/tests/preg_replace_callback_basic.phpt`
- `ext/pcre/tests/preg_replace_callback_array_trampoline.phpt`
- `ext/pcre/tests/preg_replace_callback_array_numeric_index_error.phpt`
- `ext/pcre/tests/preg_replace_callback_array_fatal_error.phpt`
- `ext/pcre/tests/preg_replace_callback_array_error.phpt`
- `ext/pcre/tests/preg_replace_callback_array2.phpt`
- `ext/pcre/tests/preg_replace_callback_array.phpt`
- `ext/pcre/tests/preg_replace_callback2.phpt`
- `ext/pcre/tests/preg_replace_callback.phpt`
- `ext/pcre/tests/preg_replace_basic.phpt`
- `ext/pcre/tests/preg_replace2.phpt`
- `ext/pcre/tests/preg_replace.phpt`
- `ext/pcre/tests/preg_quote_basic.phpt`
- `ext/pcre/tests/preg_match_non_capture.phpt`
- `ext/pcre/tests/preg_match_latin.phpt`
- `ext/pcre/tests/preg_match_frameless_leak.phpt`
- `ext/pcre/tests/preg_match_error4.phpt`
- `ext/pcre/tests/preg_match_error3.phpt`
- `ext/pcre/tests/preg_match_error1.phpt`
- `ext/pcre/tests/preg_match_caseless_restrict.phpt`
- `ext/pcre/tests/preg_match_basic_edit.phpt`
- `ext/pcre/tests/preg_match_basic.phpt`
- `ext/pcre/tests/preg_match_all_error3.phpt`
- `ext/pcre/tests/preg_match_all_error1.phpt`
- `ext/pcre/tests/preg_match_all_edit_basic.phpt`

## Relevant php-src Source Areas

- `ext/pcre/tests/`

## Target Gates

- `nix develop -c just phpt-module MODULE=pcre`

## Known Gaps

- `runtime-output-mismatch`: 67
- `runtime-error-or-diagnostic`: 43
- `runtime-unsupported-feature`: 15
- `runtime-timeout`: 1

## Next Step

Use PCRE2 while documenting unsupported modifier/callout gaps.
