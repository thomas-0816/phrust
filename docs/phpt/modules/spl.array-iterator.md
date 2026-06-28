# spl.array-iterator

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.array-iterator.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `ArrayIterator`
- `IteratorIterator`
- `RecursiveArrayIterator`
- `LimitIterator`
- `EmptyIterator`
- `AppendIterator`
- `current`, `key`, `next`, `rewind`, `valid`, `count`, `foreach`, and simple wrapping

## Non-Scope

- flags
- serialization
- live mutation edge cases
- recursive child APIs beyond selected tests

## Selected PHPT Paths

- `tests/phpt/generated/spl.array-iterator/iterator-mvps.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl.array-iterator`
- `nix develop -c just diff-spl-reflection`

## Known Gaps

- `STDLIB-GAP-SPL-ITERATOR-MUTATION-EDGES`
- `STDLIB-GAP-SPL-ITERATOR-FULL-API`

## Coverage

The selected fixture covers deterministic array-backed iteration, iterator
wrapping, limit slicing, empty iterator invalidity, append composition, and
basic recursive array iterator metadata.
