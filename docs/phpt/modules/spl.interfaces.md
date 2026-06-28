# spl.interfaces

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.interfaces.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `Countable`
- `Iterator`
- `IteratorAggregate`
- `ArrayAccess`
- `SeekableIterator`
- `RecursiveIterator`
- generated arginfo-backed method metadata reflected by selected tests

## Non-Scope

- container behavior
- exhaustive interface inheritance and reflection metadata parity

## Selected PHPT Paths

- `tests/phpt/generated/spl.interfaces/interface-method-surface.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime object`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl.interfaces`

## Known Gaps

- `STDLIB-GAP-SPL-INTERFACE-METHOD-SURFACES`

## Coverage

The selected fixture verifies interface existence, `RecursiveArrayIterator`
implementation metadata, `ArrayIterator` non-recursive interface behavior, and
`ReflectionMethod` metadata for an arginfo-backed SPL method.
