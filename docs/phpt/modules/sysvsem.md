# sysvsem PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `sysvsem` function and `SysvSemaphore` class surface
  in `php_std` and the runtime builtin registry.
- Implements `sem_get`, `sem_acquire`, `sem_release`, and `sem_remove`.
- Uses a deterministic request-local semaphore backend with max-acquire limits,
  nonblocking failure behavior, and removal semantics that do not risk CI
  deadlocks.

## Known gaps

- The backend is not cross-process System V IPC and does not allocate host
  kernel semaphores.
- Kernel auto-release, process ownership, permission checks, and blocking wait
  behavior are not modeled yet.
- The pinned php-src CLI used by this workspace does not load ext/sysvsem, so
  upstream reference PHPTs may skip on the reference side while the generated
  target fixture exercises the Phrust implementation.

## Gates

- `nix develop -c cargo test -p php_runtime sysv --no-fail-fast`
- `nix develop -c cargo test -p php_std sysv --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=sysvsem`
