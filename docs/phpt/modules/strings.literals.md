# strings.literals

- Priority: 7
- Selected manifest: `tests/phpt/manifests/modules/strings.literals.selected.jsonl`
- Current counts: 9 PASS, 0 SKIP, 0 FAIL, 0 BORK from 9 selected
  candidates

## Scope

- string literal decoding
- heredoc/nowdoc
- string interpolation basics

## Non-Scope

- full ext/standard string API

## Relevant PHPT Paths

- `tests/strings/offsets_general.phpt`
- `tests/strings/offsets_chaining_5.phpt`
- `tests/strings/offsets_chaining_3.phpt`
- `tests/strings/offsets_chaining_1.phpt`
- `tests/strings/bug26703.phpt`
- `tests/strings/bug22592.phpt`
- `tests/strings/004.phpt`
- `tests/strings/002.phpt`
- `tests/strings/001.phpt`

## Relevant php-src Source Areas

- `crates/php_lexer/`
- `crates/php_syntax/`
- `crates/php_runtime/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=strings.literals`

Last focused run on 2026-06-28:

- Selected module gate:
  `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=strings.literals`
  - Reference: 9 PASS, 0 SKIP, 0 FAIL, 0 BORK
  - Target: 9 PASS, 0 SKIP, 0 FAIL, 0 BORK
  - Source integrity: 24475 php-src manifest entries verified

## Known Gaps

The selected nine-fixture gate is green. Broader string-literal coverage should
expand heredoc/nowdoc combinations, interpolation edge cases, and additional
standard string formatting fixtures in follow-up selected slices.

## Next Step

Keep the selected literal/string gate green while expanding heredoc,
interpolation, and standard string formatting coverage.
