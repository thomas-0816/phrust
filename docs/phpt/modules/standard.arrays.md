# standard.arrays

- Priority: 12
- Selected manifest: `tests/phpt/manifests/modules/standard.arrays.selected.jsonl`
- Corpus baseline: 218 PASS, 7 SKIP, 595 FAIL, 0 BORK from 821 corpus candidates
- focused gate: 35 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- Core array builtins with stable scalar/packed-array behavior
- Promoted upstream coverage for `array_chunk`, `array_flip`,
  `array_diff_assoc`, `array_intersect`, and `array_intersect_assoc`
- Focused generated fixtures for `count`, `array_keys`, `array_values`,
  `array_merge`, `array_slice`, `array_key_exists`, `array_splice`,
  `array_column`, `in_array`, `array_search`, `array_unique`, `range`, and
  deterministic `sort`/`asort`/`ksort`

## Non-Scope

- Full upstream array corpus
- Callback-heavy helpers without VM callable dispatch
- Broad Copy-on-Write/reference behavior outside the selected fixtures

## Relevant PHPT Paths

- `tests/phpt/generated/standard.arrays/count-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-keys-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-values-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-merge-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-slice-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-key-exists-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-splice-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-column-smoke.phpt`
- `tests/phpt/generated/standard.arrays/in-array-search-smoke.phpt`
- `tests/phpt/generated/standard.arrays/array-unique-smoke.phpt`
- `tests/phpt/generated/standard.arrays/range-smoke.phpt`
- `tests/phpt/generated/standard.arrays/sort-deterministic-smoke.phpt`
- `ext/standard/tests/array/array_chunk_basic1.phpt`
- `ext/standard/tests/array/array_chunk_basic2.phpt`
- `ext/standard/tests/array/array_flip.phpt`
- `ext/standard/tests/array/array_flip_basic.phpt`
- `ext/standard/tests/array/array_flip_variation2.phpt`
- `ext/standard/tests/array/array_flip_variation3.phpt`
- `ext/standard/tests/array/array_flip_variation4.phpt`
- `ext/standard/tests/array/array_flip_variation5.phpt`
- `ext/standard/tests/array/array_diff_assoc.phpt`
- `ext/standard/tests/array/array_intersect_basic.phpt`
- `ext/standard/tests/array/array_intersect_assoc_basic.phpt`

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/arrays.rs`
- `crates/php_runtime/src/array.rs`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.arrays`
- `nix develop -c just verify-stdlib`

## Evidence

- Added generated fixtures for the remaining builtin list:
  `array_key_exists`, `array_splice`, `array_column`, `in_array`,
  `array_search`, `array_unique`, `range`, and deterministic sort coverage.
- Promoted upstream `array_chunk`, `array_flip`, `array_diff_assoc`,
  `array_intersect`, and `array_intersect_assoc` PHPTs.
- Latest focused target run: PASS, 35 selected PHPTs.
- Latest oracle-backed stdlib verification: PASS, `verify-stdlib` with
  `REFERENCE_PHP=$REFERENCE_PHP`.

## Known Gaps

- Full upstream array corpus remains larger than the selected gate.
- Callback-heavy, object, and reference-sensitive array cases need later slices
  before they can be treated as complete.
- `array_intersect_key` is not registered yet, `array_replace` still lacks
  recursion detection for the upstream endless-recursion case, and
  `array_rand` error text still needs PHP-compatible user-facing messages.
