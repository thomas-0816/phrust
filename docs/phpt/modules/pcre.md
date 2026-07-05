# pcre

- Priority: 18.6 promoted
- Selected manifest: `tests/phpt/manifests/modules/pcre.selected.jsonl`
- the selected close gate: 82 PASS, 10 SKIP, 0 FAIL, 0 BORK from 92 selected fixtures
- Previous upstream corpus snapshot before this promotion: 69 PASS, 10 SKIP, 86 FAIL, 0 BORK
  from 165 corpus candidates

## Scope

- `preg_match` captures, named captures, `PREG_OFFSET_CAPTURE`, and offset-base
  handling
- `preg_match_all` set-order capture arrays with named groups and unmatched
  nulls
- `preg_last_error` and `preg_last_error_msg` request-local state across VM
  builtin calls
- PHP paired pattern delimiters with nested delimiter bytes in groups,
  quantifiers, character classes, and named captures
- `preg_replace` simple replacements, backrefs, limits, and by-reference count
- `preg_replace` PHP replacement-token parsing for `$n`, `\n`, `${n}`,
  leading-zero tokens, and ambiguous two-digit backreferences
- `preg_split` delimiter capture and no-empty flags
- `preg_grep` positive matching
- `preg_quote` delimiter escaping
- `preg_replace_callback` named function, closure, and invalid-callable dispatch
- `preg_replace_callback` `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL`
  callback match-array flags
- `preg_replace_callback_array` sequential function and closure dispatch, count
  by reference, empty pattern maps, and array subject key preservation
- `preg_replace_callback_array` `PREG_OFFSET_CAPTURE` and
  `PREG_UNMATCHED_AS_NULL` callback match-array flags
- `preg_filter` replacement filtering with scalar `null` no-match behavior and
  array key preservation

## Non-Scope

- Remaining 83 upstream `ext/pcre` failures
- PCRE JIT, callouts, `(*MARK)`, and every PHP modifier edge
- `preg_replace_callback_array` array/method callback edge cases
- Byte-perfect warning text, stack formatting, UTF-8 edge diagnostics, and
  locale-sensitive ctype behavior

## Selected PHPT Fixtures

- `tests/phpt/generated/pcre/preg-match-captures.phpt`
- `tests/phpt/generated/pcre/preg-last-error-state.phpt`
- `tests/phpt/generated/pcre/preg-replace-split-grep-quote.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-invalid.phpt`
- `tests/phpt/generated/pcre/paired-delimiters.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-array.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-flags.phpt`
- 84 target-green upstream rows from `ext/pcre/tests`
- `ext/pcre/tests/preg_match_basic.phpt`
- `ext/pcre/tests/preg_quote_basic.phpt`
- `ext/pcre/tests/preg_replace.phpt`
- `ext/pcre/tests/preg_replace_basic.phpt`
- `ext/pcre/tests/preg_replace_callback_flags.phpt`
- `ext/pcre/tests/preg_replace_edit_basic.phpt`
- `ext/pcre/tests/preg_split_basic.phpt`
- `ext/pcre/tests/preg_grep_basic.phpt`
- `ext/pcre/tests/preg_filter.phpt`
- `ext/pcre/tests/001.phpt`
- `ext/pcre/tests/grep.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/pcre.rs`
- `crates/php_runtime/src/builtins/context.rs`
- `crates/php_runtime/src/builtins/modules/pcre.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=pcre`
- `nix develop -c cargo test -p php_runtime pcre`
- `nix develop -c cargo test -p php_vm`

## Evidence

- Replaced the broad 165-test selected manifest with a focused five-test PCRE
  harness before broad upstream promotion.
- Added generated oracle fixtures for match/match_all capture shape, PCRE
  last-error state, replace/split/grep/quote behavior, callback dispatch, and
  invalid callback diagnostics.
- PCRE last-error state is now owned by VM execution state and shared with each
  builtin context for the request.
- Capture conversion now emits named keys and absolute offsets for selected
  `preg_match` and `preg_match_all` paths.
- `preg_replace` count is bound by reference through the VM builtin argument
  bridge.
- `preg_replace` replacement expansion now follows PHP's one/two-digit
  backreference token parsing, including `${n}` braced tokens and ambiguous
  `$103`-style replacements.
- `preg_replace_callback` uses real VM callable dispatch for named functions and
  closures.
- `preg_replace_callback_array` is registered in the PCRE runtime/std surface
  and uses the VM callback dispatch path for user functions and closures.
- `preg_replace_callback` and `preg_replace_callback_array` now accept PHP 8.5's
  sixth flags argument and pass `PREG_OFFSET_CAPTURE` and
  `PREG_UNMATCHED_AS_NULL` into callback match arrays.
- Dense and sparse VM call-argument loading now suppress undefined-variable
  warnings for uninitialized callback count out-parameters before by-reference
  builtin calls.
- `preg_filter` now shares the replacement engine while filtering out subjects
  that did not match.
- The PCRE pattern parser now handles PHP paired delimiters `()`, `[]`, `{}`,
  and `<>` with nesting, escapes, character classes, and named-capture syntax.
- The selected upstream PCRE promotion now covers basic `preg_match`,
  `preg_quote`, `preg_split`, `preg_grep`, `preg_filter`, and legacy
  `preg_match_all`-free regex cases that fit the existing runtime surface.
- The previous full target-only upstream sweep on the PHP 8.5.7 oracle corpus
  measured 69 PASS, 10 SKIP, and 86 FAIL from 165 `ext/pcre/tests` rows. This
  branch promotes `preg_filter.phpt`, `preg_replace.phpt`,
  `preg_replace_basic.phpt`, `preg_replace_edit_basic.phpt`, and
  `preg_replace_callback_flags.phpt` from that failure set into the selected
  gate. The selected gate now also includes generated paired-delimiter,
  `preg_replace_callback_array`, and callback-flags coverage.

## Known Gaps

- Full upstream `ext/pcre` still has unsupported feature, warning parity,
  callback-array edge cases, UTF-8, malformed-pattern diagnostics, and
  locale-sensitive cases.
- The remaining failure set includes `preg_match_all` capture shape variants,
  `preg_replace_callback_array` array/method callback edge cases, `preg_filter`
  edge cases, replacement backreference edge cases, split edge cases, invalid
  UTF-8 offsets, recursion/backtrack limits, JIT/callout/mark cases, and
  byte-perfect warning text.
- Direct runtime-registry callback use remains limited to internal callables;
  userland callback dispatch is covered through the VM path.
- Fatal stack formatting for invalid callbacks is intentionally matched by
  regex in the selected PHPT while VM diagnostic source-span parity remains a
  broader diagnostics task.

## Next Step

Close the remaining `preg_match_all` shape, warning parity, UTF-8,
`preg_replace_callback_array` array/method callback edge cases, advanced
replacement/split, and `preg_filter` edge-case failures before the next
upstream promotion.
