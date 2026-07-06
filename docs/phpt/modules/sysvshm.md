# sysvshm PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `sysvshm` function and `SysvSharedMemory` class
  surface in `php_std` and the runtime builtin registry.
- Implements `shm_attach`, `shm_detach`, `shm_has_var`, `shm_put_var`,
  `shm_get_var`, `shm_remove_var`, and `shm_remove`.
- Uses a deterministic request-local shared variable backend keyed by segment
  and variable IDs. Values are stored with the runtime `Value` model and cloned
  on read.

## Known gaps

- The backend is not cross-process System V shared memory and does not allocate
  host kernel segments.
- Serialized storage byte accounting, permission checks, host segment IDs, and
  platform-specific errno warning text are not modeled yet.
- The pinned php-src CLI used by this workspace does not load ext/sysvshm, so
  upstream reference PHPTs may skip on the reference side while the generated
  target fixture exercises the Phrust implementation.

## Gates

- `nix develop -c cargo test -p php_runtime sysv --no-fail-fast`
- `nix develop -c cargo test -p php_std sysv --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=sysvshm`
