# spl.array-object

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.array-object.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- construct from array
- `ArrayAccess` basics
- `Countable`
- `foreach`
- `getArrayCopy`
- `exchangeArray`
- `append`
- `offsetExists`, `offsetGet`, `offsetSet`, and `offsetUnset`

## Non-Scope

- flags
- object property mode edge cases
- serialization
- nested by-reference `ArrayAccess` writes

## Selected PHPT Paths

- `tests/phpt/generated/spl.array-object/array-object-mvp.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl.array-object`

## Known Gaps

- `STDLIB-GAP-SPL-CONTAINER-FULL-API`
- `STDLIB-GAP-SPL-CONTAINER-NESTED-ARRAYACCESS`

## Coverage

The selected fixture covers array construction, count, `foreach`, direct
offset operations, `append`, and replacing storage with `exchangeArray`.
