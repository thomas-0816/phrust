# spl.object-storage

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.object-storage.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `offsetSet`
- `offsetUnset`
- `offsetExists`
- `count`
- `current`, `key`, `next`, `rewind`, and `valid`
- `foreach`
- simple object identity keys via `ObjectRef::id()`

## Non-Scope

- info data edge cases
- serialization
- object-key bracket syntax edge cases if the lvalue model cannot express them

## Selected PHPT Paths

- `tests/phpt/generated/spl.object-storage/object-storage-mvp.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c just phpt-dev-module MODULE=spl.object-storage`

## Known Gaps

- `STDLIB-GAP-SPL-OBJECT-HASH-PARITY`
- `STDLIB-GAP-SPL-CONTAINER-FULL-API`

## Coverage

The selected fixture verifies object-identity keyed storage with two distinct
objects, info lookup through `offsetGet`, deterministic foreach order, and
`offsetUnset` count updates.
