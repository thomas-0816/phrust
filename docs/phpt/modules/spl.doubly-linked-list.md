# spl.doubly-linked-list

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.doubly-linked-list.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `SplDoublyLinkedList`
- `SplStack`
- `SplQueue`
- `push`, `pop`, `shift`, `unshift`
- `top`, `bottom`
- `count`
- simple-order `foreach`

## Non-Scope

- full iterator mode matrix
- serialization
- exact exception text for every edge

## Selected PHPT Paths

- `tests/phpt/generated/spl.doubly-linked-list/linear-containers-mvp.phpt`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=spl.doubly-linked-list`
- `nix develop -c just diff-spl-reflection`

## Known Gaps

- `STDLIB-GAP-SPL-CONTAINER-FULL-API`

## Coverage

The selected fixture covers shared linear storage behavior across list, stack,
and queue classes, including subclass metadata for stack and queue.
