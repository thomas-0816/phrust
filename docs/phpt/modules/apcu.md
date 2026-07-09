# apcu PHPT module status

## Scope

- `apcu_enabled`.
- Request-local `apcu_store`, `apcu_add`, `apcu_fetch`, `apcu_exists`,
  `apcu_delete`, and `apcu_clear_cache`.
- TTL expiry for request-local entries.
- Existing-integer counters with `apcu_inc` and `apcu_dec`.
- Deterministic probe shapes for `apcu_cache_info` and `apcu_sma_info`.

## Non-scope

- Shared process cache persistence across independent VM executions.
- `APCUIterator` traversal.
- Multi-key array fetch/delete/exists semantics.
- Per-entry shared-memory accounting.

## Selected tests

- `tests/phpt/generated/apcu/basic.phpt`
- `tests/phpt/generated/apcu/ttl-clear.phpt`

## Verification

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_TIMEOUT_SECONDS=20 PHPT_WORK_DIR=/private/tmp/phrust-phpt-apcu-selected-ttl-clear-rerun nix develop -c just phpt-dev-module MODULE=apcu`
  - Reference: PASS 2, non-green 0.
  - Target: PASS 2, non-green 0.
  - php-src manifest integrity: verified 24475 entries, skipped 0
    host-generated entries.
- `nix develop -c cargo test -q -p php_runtime builtins::modules::apcu::tests`
  - PASS: 1 test.
- `nix develop -c cargo test -q -p php_std apcu`
  - PASS: 0 selected tests, command completed successfully.
