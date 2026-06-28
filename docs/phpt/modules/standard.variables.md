# standard.variables

- Priority: 15
- Selected manifest: `tests/phpt/manifests/modules/standard.variables.selected.jsonl`
- Prompt 16.1 baseline: 23 PASS, 74 SKIP, 348 FAIL, 0 BORK from 446 corpus candidates
- Prompt 16.9 focused gate: 26 PASS, 1 SKIP, 0 FAIL, 0 BORK

## Scope

- Variable inspection and conversion builtins covered by the selected focused
  gate

## Non-Scope

- General VM symbol-table redesign
- Complete object/reference rendering matrix

## Relevant PHPT Paths

- `tests/phpt/generated/standard.variables/`
- Selected upstream `ext/standard/tests/array/` and
  `ext/standard/tests/general_functions/` cases in the manifest

## Relevant Source Areas

- `crates/php_runtime/src/builtins/modules/core.rs`
- `crates/php_runtime/src/value.rs`
- `crates/php_runtime/src/object/`
- `crates/php_vm/src/vm/mod.rs`

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=standard.variables`
- `nix develop -c just verify-stdlib`

## Prompt 16 Evidence

- Kept the selected variable-inspection slice green after the standard-core
  follow-up changes.
- Latest focused target run: PASS, 27 selected PHPTs with 26 PASS and 1 SKIP.

## Known Gaps

- Full `var_dump`/`print_r` object visibility, magic behavior, and reference
  formatting remain outside this selected gate.
