# standard.arrays

- Priority: 12
- Selected manifest: `tests/phpt/manifests/modules/standard.arrays.selected.jsonl`
- Prompt 16.1 baseline: 218 PASS, 7 SKIP, 595 FAIL, 0 BORK from 821 corpus candidates
- Prompt 16.9 focused gate: 10 PASS, 0 FAIL, 0 BORK

## Scope

- Core array builtins with stable scalar/packed-array behavior
- Focused generated fixtures for `count`, `array_keys`, `array_values`,
  `array_merge`, and `array_slice`

## Non-Scope

- Full upstream array corpus
- Comparator sorting and callback-heavy helpers
- Broad Copy-on-Write/reference behavior outside the selected fixtures

## Relevant PHPT Paths

- `tests/phpt/generated/standard.arrays/count-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-keys-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-values-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-merge-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-slice-smoke.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/arrays.rs`
- `crates/php_runtime/src/array.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.arrays`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Added focused generated array fixtures and selected-manifest coverage.
- Added VM array-cast behavior for arrays, null/uninitialized values, objects,
  and scalar/resource values.
- Latest focused target run: PASS, 10 selected PHPTs.

## Known Gaps

- Full upstream array corpus remains larger than the focused Prompt 16 gate.
- Callback, sorting, object, and reference-sensitive array cases need later
  slices before they can be treated as complete.
