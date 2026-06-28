# opcache

- Strategy: classify, do not implement
- Classification: out-of-scope
- Selected manifest: `tests/phpt/manifests/modules/opcache.selected.jsonl`
- Current corpus snapshot: 593 `opcache` candidates, 220 PASS, 8 SKIP, 364
  FAIL, 0 BORK, and 449 known non-green outcomes.

## Decision

Do not implement Opcache or JIT for PHPT compatibility in this branch.

Opcache is a production cache/optimizer/JIT subsystem. PHPTs that require
cache state, optimizer passes, file update protection, preloading, or JIT
behavior are out of scope. Ordinary PHP behavior covered by opcache-located
tests should be routed to the owning runtime/front-end module when selected.

## Unsupported Area

- Stable ID: `PHPT-DATA-OPCACHE`
- Reference behavior: PHP with Opcache enabled exposes `opcache_*` functions,
  INI-controlled cache state, invalidation, preloading, optimizer behavior, and
  JIT controls.
- Current phrust behavior: `extension_loaded("opcache")` is false, opcache INI
  options are accepted only as inert CLI options where already supported, and no
  cache/JIT subsystem exists. The pinned PHP oracle exposes some opcache
  function symbols even while `extension_loaded("opcache")` is false, so the
  selected platform probe asserts extension availability rather than function
  symbol parity.
- Fixture: `tests/phpt/generated/opcache/platform-checks.phpt`
- Next owner layer: optional performance/cache layer, not the PHP-visible core
  runtime.

## Policy

- Opcache replacement: out-of-scope.
- JIT PHPTs: out-of-scope unless minimized to ordinary PHP behavior in the
  owning module.
- Preload/cache invalidation PHPTs: out-of-scope.

## Source References

- `ext/opcache/opcache.stub.php`
- `ext/opcache/tests/`
- `ext/opcache/tests/jit/`

## Target Gates

- `nix develop -c just phpt-dev-module MODULE=opcache`
- `nix develop -c just verify-phpt`
