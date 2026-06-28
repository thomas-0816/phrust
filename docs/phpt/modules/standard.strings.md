# standard.strings

- Priority: 13
- Selected manifest: `tests/phpt/manifests/modules/standard.strings.selected.jsonl`
- Prompt 16.1 baseline: 352 PASS, 42 SKIP, 308 FAIL, 0 BORK from 727 corpus candidates
- Prompt 16.9 focused gate: 15 PASS, 0 FAIL, 0 BORK

## Scope

- Common binary-safe string helpers
- Focused generated fixtures for length, substring, search, trimming,
  split/join, formatted output, and tokenizer state

## Non-Scope

- Full upstream string corpus
- Complete formatting matrix
- Charset/encoding-heavy behavior

## Relevant PHPT Paths

- `tests/phpt/generated/standard.strings/strlen-substr-binary-smoke.phpt`
- `tests/phpt/generated/standard.strings/strpos-contains-smoke.phpt`
- `tests/phpt/generated/standard.strings/trim-explode-implode-smoke.phpt`
- `tests/phpt/generated/standard.strings/printf-sprintf-smoke.phpt`
- `tests/phpt/generated/standard.strings/strtok-state-smoke.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/strings.rs`
- `crates/php_runtime/src/value.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.strings`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Narrowed the selected manifest to the green focused string slice.
- Fixed `explode` empty-separator diagnostics and `implode` array string-cast
  behavior for nested arrays.
- Routed generated arginfo deprecations through VM diagnostics so
  `error_reporting` suppresses them consistently.
- Latest focused target run: PASS, 15 selected PHPTs.

## Known Gaps

- The full upstream string corpus remains larger than this focused slice.
- Additional formatting, encoding, flag, and uncommon helper behavior remains
  backlog work.
