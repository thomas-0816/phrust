# standard.output

- Priority: 16.6
- Selected manifest: `tests/phpt/manifests/modules/standard.output.selected.jsonl`
- Prompt 16.1 derived baseline: 11 PASS, 0 SKIP, 63 FAIL, 20 BORK from 94 path-filtered candidates
- Prompt 16.9 focused gate: 11 PASS, 0 FAIL, 0 BORK

## Scope

- Stack-backed output buffering basics
- `ob_start`, `ob_get_clean`, nested buffers, clean, flush, and basic state
  transitions covered by the selected gate

## Non-Scope

- Output-buffer callback handlers
- Chunk sizes, handler flags, and handler-list APIs
- Full `tests/output/` corpus

## Relevant PHPT Paths

- `tests/output/ob_start_basic_001.phpt`
- `tests/output/ob_002.phpt`
- `tests/output/ob_005.phpt`
- `tests/output/ob_006.phpt`
- `tests/output/ob_007.phpt`
- `tests/output/ob_008.phpt`
- `tests/output/ob_get_clean_basic_001.phpt`
- `tests/output/ob_get_clean_basic_002.phpt`
- `tests/phpt/generated/standard.output/output-buffer-basic-state.phpt`
- `tests/phpt/generated/standard.output/output-buffer-nested-clean.phpt`
- `tests/phpt/generated/standard.output/output-buffer-clean-flush.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/output.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.output`
- `nix develop -c cargo test -p php_vm`

## Prompt 16 Evidence

- Added a dedicated selected manifest and generated smoke fixtures for the
  output-buffer MVP.
- No runtime code changes were needed for the focused gate; existing VM output
  stack behavior matched the selected reference cases.
- Latest focused target run: PASS, 11 selected PHPTs.

## Known Gaps

- Callback, chunk-size, flag, and handler-list semantics remain outside this
  focused selected gate.
