# pcre

- Priority: 18.6 promoted
- Selected manifest: `tests/phpt/manifests/modules/pcre.selected.jsonl`
- the selected close gate: 163 PASS, 10 SKIP, 0 FAIL, 0 BORK from 173 selected fixtures
- Latest selected module gate after this promotion: target 163 PASS / 10 SKIP;
  0 non-green outcomes. The three extra skips are upstream slow/performance
  fixtures gated by `SKIP_SLOW_TESTS=1` and `SKIP_PERF_SENSITIVE=1`.

## Scope

- `preg_match` captures, named captures, `PREG_OFFSET_CAPTURE`, and offset-base
  handling
- `preg_match` and `preg_match_all` negative offset normalization with absolute
  capture offsets and positive out-of-range empty-match initialization
- `preg_match` and `preg_match_all` `PHP_INT_MIN` offset `ValueError` parity
- `preg_match` and `preg_match_all` `/u` offset validation for offsets that
  fall inside a valid UTF-8 code point, including `PREG_BAD_UTF8_OFFSET_ERROR`
  and `preg_last_error_msg()` state
- `preg_match` and `preg_match_all` `/u` matching after invalid UTF-8 prefixes
  when the requested offset starts a valid suffix, preserving absolute capture
  offsets
- `preg_match`, `preg_match_all`, and `preg_replace_callback` PCRE
  `(*MARK:...)` match-array parity, including sparse pattern-order `MARK`
  groups
- `preg_match`, `preg_replace`, `preg_split`, and `preg_grep` malformed UTF-8
  subject handling for `/u` patterns, including `PREG_BAD_UTF8_ERROR`,
  canonical last-error message text, and PHP return values
- `preg_match_all` backtrack and recursion/depth limit failures from
  request-local `pcre.backtrack_limit`, `pcre.recursion_limit`, and `pcre.jit`
  INI settings
- `preg_match_all` non-variable matches arguments emit PHP's by-reference fatal
  shape, including the blank diagnostic separator after prior output
- `preg_split` and `preg_match_all` UTF-8 character iteration with
  `htmlentities` entity rendering for selected non-ASCII characters
- `preg_replace` and `preg_match_all` UTF-8 lookbehind behavior
- `preg_match_all` set-order capture arrays with named groups and unmatched
  nulls
- `preg_match` and `preg_match_all` `PREG_*` flag validation
- `preg_match` and `preg_match_all` named duplicate capture precedence, named
  assignment stability, and trailing unmatched capture trimming
- `preg_match` and `preg_match_all` pattern argument `TypeError` parity for
  array and non-stringable object values
- `preg_match_all`, `preg_replace`, and `preg_split` zero-width `\K`
  bump-along handling
- `preg_last_error` and `preg_last_error_msg` request-local state across VM
  builtin calls
- PCRE version constants and `PCRE_JIT_SUPPORT` matching the PHP 8.5.7 oracle
- PHP-compatible PCRE JIT stack sizing for JIT stack-limit error parity
- PCRE request-shutdown safety when user stream wrapper `stream_close()`
  callbacks call `preg_*`
- PHP paired pattern delimiters with nested delimiter bytes in groups,
  quantifiers, character classes, and named captures
- `preg_match` malformed-pattern warnings for empty patterns, invalid
  delimiters, missing closing delimiters, unknown modifiers, and
  `preg_last_error_msg()` internal-error state
- `preg_match` stringable pattern and subject coercion with `@`-suppressed
  malformed-pattern warnings
- `preg_match` whitespace delimiter and empty-pattern diagnostics
- `preg_match` NUL-byte delimiter and modifier diagnostics
- PCRE2 compile-warning text normalized to PHP's
  `Compilation failed: ... at offset ...` form
- PHP delimiter modifiers `A`, `D`, `J`, `n`, `r`, `U`, `S`, and `X`, plus
  whitespace modifier tails, for anchored, dollar-end-only, duplicate-name,
  no-auto-capture, caseless-restrict, ungreedy, and study/no-op matching
- `preg_replace` simple replacements, backrefs, limits, and by-reference count
- `preg_replace` PHP replacement-token parsing for `$n`, `\n`, `${n}`,
  leading-zero tokens, and ambiguous two-digit backreferences
- `preg_replace` invalid-pattern warnings with `null` return parity
- `preg_replace` malformed-character-class compile-error parity
- `preg_replace` array-pattern and array-replacement type parity, including
  arrays of failing patterns
- `preg_split` delimiter capture, no-empty flags, recursion-limit failures,
  empty delimiter captures, and zero-width `\K` limit handling
- `preg_grep` positive matching and array element string-cast warnings without
  input mutation
- `preg_quote` delimiter and NUL-byte escaping
- `preg_replace_callback` named function, closure, invalid-callable dispatch,
  array-pattern dispatch, nested-array subject string-cast warnings, and
  runtime redeclaration fatal output
- `preg_replace_callback` invalid-pattern warnings with `null` return parity
- `preg_replace_callback` `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL`
  callback match-array flags
- `preg_replace_callback_array` sequential function and closure dispatch, count
  by reference, empty pattern maps, and array subject key preservation
- `preg_replace_callback_array` invalid-pattern warnings, callback-map errors,
  callback-key `TypeError` fatal formatting, and `null` return parity
- `preg_replace_callback_array` `PREG_OFFSET_CAPTURE` and
  `PREG_UNMATCHED_AS_NULL` callback match-array flags
- `preg_filter` replacement filtering with scalar `null` no-match behavior and
  array key preservation

## Non-Scope

- PCRE callout parity
- Remaining byte-perfect warning/path text, stack formatting, malformed-pattern
  diagnostics, and locale-sensitive ctype behavior

## Selected PHPT Fixtures

- `tests/phpt/generated/pcre/preg-match-captures.phpt`
- `tests/phpt/generated/pcre/preg-last-error-state.phpt`
- `tests/phpt/generated/pcre/preg-replace-split-grep-quote.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-invalid.phpt`
- `tests/phpt/generated/pcre/paired-delimiters.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-array.phpt`
- `tests/phpt/generated/pcre/preg-replace-callback-flags.phpt`
- 161 selected upstream rows from `ext/pcre/tests`
- `ext/pcre/tests/preg_match_basic.phpt`
- `ext/pcre/tests/preg_match_error1.phpt`
- `ext/pcre/tests/002.phpt`
- `ext/pcre/tests/003.phpt`
- `ext/pcre/tests/005.phpt`
- `ext/pcre/tests/errors03.phpt`
- `ext/pcre/tests/errors04.phpt`
- `ext/pcre/tests/preg_match_all_basic.phpt`
- `ext/pcre/tests/preg_match_all_error1.phpt`
- `ext/pcre/tests/delimiters.phpt`
- `ext/pcre/tests/null_bytes.phpt`
- `ext/pcre/tests/preg_quote_basic.phpt`
- `ext/pcre/tests/preg_replace.phpt`
- `ext/pcre/tests/preg_replace_basic.phpt`
- `ext/pcre/tests/preg_replace_callback_flags.phpt`
- `ext/pcre/tests/preg_replace_callback2.phpt`
- `ext/pcre/tests/bug44925.phpt`
- `ext/pcre/tests/preg_replace_edit_basic.phpt`
- `ext/pcre/tests/preg_split_basic.phpt`
- `ext/pcre/tests/preg_grep_basic.phpt`
- `ext/pcre/tests/preg_filter.phpt`
- `ext/pcre/tests/bug70232.phpt`
- `ext/pcre/tests/bug70345.phpt`
- `ext/pcre/tests/bug70345_old.phpt`
- `ext/pcre/tests/bug41638.phpt`
- `ext/pcre/tests/bug74183.phpt`
- `ext/pcre/tests/bug75089.phpt`
- `ext/pcre/tests/bug75207.phpt`
- `ext/pcre/tests/bug76127.phpt`
- `ext/pcre/tests/bug77827.phpt`
- `ext/pcre/tests/dollar_endonly.phpt`
- `ext/pcre/tests/grep2.phpt`
- `ext/pcre/tests/pcre_anchored.phpt`
- `ext/pcre/tests/preg_match_caseless_restrict.phpt`
- `ext/pcre/tests/preg_grep_error1.phpt`
- `ext/pcre/tests/preg_match_frameless_leak.phpt`
- `ext/pcre/tests/preg_match_non_capture.phpt`
- `ext/pcre/tests/preg_split_error1.phpt`
- `ext/pcre/tests/errors06.phpt`
- `ext/pcre/tests/errors02.phpt`
- `ext/pcre/tests/invalid_utf8.phpt`
- `ext/pcre/tests/invalid_utf8_offset.phpt`
- `ext/pcre/tests/bug37911.phpt`
- `ext/pcre/tests/bug73392.phpt`
- `ext/pcre/tests/preg_replace_callback_array_error.phpt`
- `ext/pcre/tests/preg_replace_error1.phpt`
- `ext/pcre/tests/preg_replace_callback_error1.phpt`
- `ext/pcre/tests/preg_replace_callback_fatal_error_leak.phpt`
- `ext/pcre/tests/preg_replace_error2.phpt`
- `ext/pcre/tests/bug21732.phpt`
- `ext/pcre/tests/bug26927.phpt`
- `ext/pcre/tests/bug27103.phpt`
- `ext/pcre/tests/bug34790.phpt`
- `ext/pcre/tests/bug53823.phpt`
- `ext/pcre/tests/bug61780_2.phpt`
- `ext/pcre/tests/bug66121.phpt`
- `ext/pcre/tests/bug79241.phpt`
- `ext/pcre/tests/bug79257.phpt`
- `ext/pcre/tests/bug81424a.phpt`
- `ext/pcre/tests/gh16189.phpt`
- `ext/pcre/tests/backtrack_limit.phpt`
- `ext/pcre/tests/recursion_limit.phpt`
- `ext/pcre/tests/request47456.phpt`
- `ext/pcre/tests/study.phpt`
- `ext/pcre/tests/ungreedy.phpt`
- `ext/pcre/tests/001.phpt`
- `ext/pcre/tests/grep.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/pcre.rs`
- `crates/php_runtime/src/builtins/context.rs`
- `crates/php_runtime/src/builtins/modules/pcre.rs`
- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_ir/src/lower/expressions.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `SKIP_SLOW_TESTS=1 SKIP_PERF_SENSITIVE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=pcre`
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
- `preg_match` and `preg_match_all` now normalize negative offsets using PHP's
  byte-offset rules, including clamping before-start offsets to zero and
  initializing the matches argument to an empty array for positive out-of-range
  offsets.
- `preg_match` and `preg_match_all` now reject `/u` offsets inside valid UTF-8
  code points with `PREG_BAD_UTF8_OFFSET_ERROR`, empty match output, and PHP's
  bad-offset last-error message.
- `preg_match` and `preg_match_all` now validate malformed `/u` subjects from
  the requested offset, allowing matches after an invalid UTF-8 prefix when the
  suffix is valid while preserving absolute capture offsets.
- `preg_match`, `preg_replace`, `preg_split`, and `preg_grep` now reject
  malformed UTF-8 subjects for `/u` patterns with PHP's
  `PREG_BAD_UTF8_ERROR` state and return-family parity (`false` for match,
  split, and grep; `null` for replacement).
- PCRE match array conversion now preserves PHP's named duplicate capture
  precedence, trims trailing unmatched captures unless
  `PREG_UNMATCHED_AS_NULL` is requested, and retains named pattern-order rows.
- `preg_replace` count is bound by reference through the VM builtin argument
  bridge.
- `preg_replace` replacement expansion now follows PHP's one/two-digit
  backreference token parsing, including `${n}` braced tokens and ambiguous
  `$103`-style replacements.
- `preg_replace_callback` uses real VM callable dispatch for named functions and
  closures.
- `preg_replace_callback_array` is registered in the PCRE runtime/std surface
  and uses the VM callback dispatch path for user functions and closures.
- Invalid `preg_replace_callback_array` pattern-map keys and callback entries
  now route through VM `TypeError` throwable handling, preserving catchability
  while rendering PHP-shaped uncaught fatal output and call traces.
- `preg_replace_callback` and `preg_replace_callback_array` now accept PHP 8.5's
  sixth flags argument and pass `PREG_OFFSET_CAPTURE` and
  `PREG_UNMATCHED_AS_NULL` into callback match arrays.
- `preg_replace_callback` now uses PHP weak string casting for nested array
  subjects, emitting `Array to string conversion` while continuing with the
  literal `Array` string.
- Dense and sparse VM call-argument loading now suppress undefined-variable
  warnings for uninitialized callback count out-parameters before by-reference
  builtin calls.
- VM builtin argument loading now pre-coerces selected string parameters through
  PHP object stringification, covering stringable `preg_match` pattern and
  subject objects before PCRE pattern parsing.
- Non-variable `@` expressions now lower through a temporary
  `error_reporting(0)` scope and restore the previous reporting mask after the
  expression, covering suppressed `preg_match` compile warnings.
- `preg_filter` now shares the replacement engine while filtering out subjects
  that did not match.
- `preg_quote` now escapes NUL bytes as PHP's `\000` octal escape while still
  returning a pattern that matches the original binary string.
- The PCRE pattern parser now handles PHP paired delimiters `()`, `[]`, `{}`,
  and `<>` with nesting, escapes, character classes, and named-capture syntax.
- Malformed PCRE pattern compilation now emits PHP warnings at the preg call
  site while preserving the request-local `preg_last_error_msg()` value used by
  PHP for internal compile errors.
- PCRE2 compile errors now use PHP warning wording instead of leaking the raw
  `PCRE2: error compiling pattern...` string.
- PHP whitespace delimiters now follow php-src: all-whitespace pattern strings
  report `Empty regular expression`, while non-empty whitespace-delimited
  strings fail delimiter validation.
- NUL bytes in the modifier tail now emit PHP's dedicated
  `NUL byte is not a valid modifier` warning instead of falling through to the
  generic unknown-modifier warning.
- PHP delimiter modifiers now cover `A`, `D`, `J`, `n`, `r`, `U`, `S`, and the
  legacy ignored `X` modifier, plus whitespace modifier tails, by mapping
  anchored, duplicate-name,
  no-auto-capture, caseless-restrict, and ungreedy behavior into PCRE2 compile
  options, preserving study/no-op mode, and rewriting `$` anchors for
  dollar-end-only mode.
- The selected upstream PCRE promotion now covers basic `preg_match`,
  `preg_quote`, `preg_split`, `preg_grep`, `preg_filter`, and legacy
  `preg_match_all`-free regex cases that fit the existing runtime surface.
- Runtime function redeclarations triggered inside `preg_replace_callback`
  callbacks now emit PHP-shaped fatal output with the previous declaration site.
- The current full module gate with reference and target reuse disabled runs 173
  selected rows green on both engines: reference 166 PASS / 7 SKIP and target
  166 PASS / 7 SKIP. This branch promotes `errors03.phpt`, compile-warning rows
  such as
  `bug70345.phpt`, `bug70345_old.phpt`, `grep2.phpt`, `preg_grep_error1.phpt`, and
  `preg_split_error1.phpt`, modifier rows such as `pcre_anchored.phpt`,
  `dollar_endonly.phpt`, `preg_match_caseless_restrict.phpt`,
  `preg_match_non_capture.phpt`, `ungreedy.phpt`, and `bug41638.phpt`,
  whitespace modifier-tail rows `bug77827.phpt`, `study.phpt`, and
  `pcre_extra.phpt`, plus
  UTF-8 offset rows `errors06.phpt` and `invalid_utf8_offset.phpt`,
  replacement-family compile-error rows `bug37911.phpt`, `bug73392.phpt`,
  `preg_replace_callback_array_error.phpt`, `preg_replace_error1.phpt`, and
  `preg_replace_callback_error1.phpt`,
  replacement-family type-error rows `preg_replace_error2.phpt` and
  `bug21732.phpt`,
  named capture shape rows `003.phpt`, `005.phpt`, `bug34790.phpt`,
  `bug61780_2.phpt`, `bug79257.phpt`, `bug81424a.phpt`, and
  `request47456.phpt`,
  flag and malformed-pattern rows `002.phpt`, `delimiters.phpt`, and
  `null_bytes.phpt`,
  malformed UTF-8 subject rows `bug53823.phpt`, `bug75089.phpt`,
  `bug76127.phpt`, `errors02.phpt`, and `invalid_utf8.phpt`, UTF-8 lookbehind
  row `bug66121.phpt`, match-limit rows `backtrack_limit.phpt`,
  `recursion_limit.phpt`, and `errors04.phpt`,
  PCRE mark row `marks.phpt`,
  `preg_match_all_basic.phpt`, `preg_match_error1.phpt`,
  `preg_match_all_error1.phpt`, `preg_match_all_error3.phpt`,
  `preg_filter.phpt`, `preg_replace.phpt`,
  `preg_replace_basic.phpt`, `preg_replace_edit_basic.phpt`, and
  `preg_replace_callback_flags.phpt`, plus `006.phpt`, `bug52732.phpt`,
  `bug80866.phpt`, `errors01.phpt`,
  `preg_replace_callback_array2.phpt`, `preg_replace_callback2.phpt`,
  `preg_replace_callback_array_fatal_error.phpt`,
  `preg_replace_callback_array_numeric_index_error.phpt`,
  `preg_replace_callback_fatal_error_leak.phpt`,
  `split.phpt`, `split2.phpt`, `bug70232.phpt`, `gh16189.phpt`,
  `bug79241.phpt`, `bug26927.phpt`, `bug27103.phpt`, `bug44925.phpt`,
  `preg_match_frameless_leak.phpt`, `bug76514.phpt`,
  `preg_match_error4.phpt`, `gh15205_2.phpt`, and `bug72685.phpt` into the selected gate.
  The selected gate now also includes generated
  paired-delimiter, `preg_replace_callback_array`, callback-flags coverage,
  and the `preg_replace_callback*` trampoline rows.

## Known Gaps

- Further PCRE work remains for callout parity, byte-perfect warning text,
  malformed-pattern diagnostic coverage, and locale-sensitive ctype behavior.
- `bug72685.phpt` is promoted after the repeated UTF-8 validation fast path
  passed the focused upstream PHPT gate.
- Direct runtime-registry callback use remains limited to internal callables;
  userland callback dispatch is covered through the VM path.
- Fatal stack formatting for invalid callbacks is intentionally matched by
  regex in the selected PHPT while VM diagnostic source-span parity remains a
  broader diagnostics task.

## Next Step

Keep the full `pcre` module gate green while expanding callout, byte-perfect
warning text, malformed-pattern diagnostic, and locale-sensitive ctype coverage.
