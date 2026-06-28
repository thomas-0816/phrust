# spl.fixed-array

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.fixed-array.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- construct with size
- `count`
- `offsetExists`, `offsetGet`, `offsetSet`, and `offsetUnset`
- `setSize` and `getSize`
- `toArray`
- `foreach`

## Non-Scope

- full exception wording
- serialization

## Selected PHPT Paths

- `tests/phpt/generated/spl.fixed-array/fixed-array-mvp.phpt`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=spl.fixed-array`
- `nix develop -c just diff-spl-reflection`

## Known Gaps

- `STDLIB-GAP-SPL-CONTAINER-FULL-API`

## Coverage

The selected fixture covers fixed-size construction, nullable slots, offset
write/read/unset, array conversion, resizing, and iteration order.
