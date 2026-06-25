# diagnostics.output

- Priority: 6
- Selected manifest: `tests/phpt/manifests/modules/diagnostics.output.selected.jsonl`
- Current counts: 0 PASS, 0 SKIP, 0 FAIL, 0 BORK from 0 corpus candidates

## Scope

- warnings
- notices
- fatal formatting
- display_errors
- output channels

## Non-Scope

- exact wording for intentionally unsupported extensions

## Relevant PHPT Paths

- none identified yet

## Relevant php-src Source Areas

- `crates/php_runtime/`
- `crates/php_vm/`

## Target Gates

- `nix develop -c just verify-runtime`

## Known Gaps

- no known non-green fingerprints assigned in the current baseline

## Next Step

Centralize runtime diagnostic rendering and continuation semantics.
