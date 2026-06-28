# spl.autoload

- Priority: 20
- Selected manifest: `tests/phpt/manifests/modules/spl.autoload.selected.jsonl`
- Current selected counts: 1 PASS, 0 SKIP, 0 FAIL, 0 BORK

## Scope

- `spl_autoload_register`
- `spl_autoload_unregister`
- `spl_autoload_functions`
- callback order
- string/function callbacks
- closure callbacks
- class lookup invokes autoload

## Non-Scope

- full prepend/throw exactness unless selected PHPTs require it
- default `spl_autoload` namespace/path conventions

## Selected PHPT Paths

- `tests/phpt/generated/spl.autoload/autoload-mvp.phpt`

## Target Gates

- `nix develop -c cargo test -p php_runtime autoload`
- `nix develop -c cargo test -p php_vm`
- `nix develop -c just phpt-dev-module MODULE=spl.autoload`

## Known Gaps

- `STDLIB-GAP-SPL-AUTOLOAD-ADVANCED`

## Coverage

The selected fixture covers function and closure registration, callback order,
autoload on missing class lookup, callback listing count, unregistering a
function callback, and explicit `spl_autoload_call`.
