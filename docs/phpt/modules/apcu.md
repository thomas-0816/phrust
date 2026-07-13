# apcu PHPT module status

## Scope

- `apcu_enabled`.
- Process-local `apcu_store`, `apcu_add`, `apcu_fetch`, `apcu_exists`,
  `apcu_delete`, and `apcu_clear_cache`.
- `apcu_entry` through VM-mediated callable execution.
- TTL expiry for process-local entries.
- Existing-integer counters with `apcu_inc` and `apcu_dec`; shared cache
  handles lock only for the cache operation.
- Deterministic probe shapes for `apcu_cache_info` and `apcu_sma_info`.

## Non-scope

- Cross-thread APCu sharing. Runtime values are thread-affine, so the default
  cache is process-local within the runtime thread and independent OS threads
  keep separate stores.
- `APCUIterator` traversal.
- Multi-key array fetch/delete/exists semantics.
- Per-entry shared-memory accounting.

## Selected tests

- `tests/phpt/generated/apcu/basic.phpt`
- `tests/phpt/generated/apcu/ttl-clear.phpt`

## Verification

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=apcu`
  - Reference: SKIP 2 because APCu is not loaded in the local php-src oracle.
  - Target: PASS 2, non-green 0.
  - php-src manifest integrity: verified 24468 entries, skipped 7
    host-generated entries.
- `nix develop -c cargo test -p php_runtime apcu -- --nocapture`
  - PASS: 3 tests.
- `nix develop -c cargo test -p php_std apcu -- --nocapture`
  - PASS: 0 selected tests, command completed successfully.
