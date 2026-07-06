# shmop PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `shmop` function and `Shmop` class surface in
  `php_std` and the runtime builtin registry.
- Implements `shmop_open`, `shmop_read`, `shmop_write`, `shmop_size`,
  `shmop_delete`, and deprecated `shmop_close`.
- Uses a deterministic request-local in-memory backend for shared-memory
  segments. Keyed opens share handles inside one request, while key `0` creates
  isolated private segments.
- Preserves binary-safe read and write behavior, read-only attach mode, and
  request-local delete semantics without leaking host SysV shared-memory
  segments during tests.

## Known gaps

- The backend is not cross-process shared memory and does not allocate real
  SysV segments.
- Platform errno-specific warning text is bounded to deterministic warnings and
  errors rather than exact libc strings.
- The pinned php-src CLI used by this workspace does not load ext/shmop, so
  upstream reference PHPTs may skip on the reference side while the generated
  target fixture exercises the Phrust implementation.

## Gates

- `nix develop -c cargo test -p php_runtime shmop --no-fail-fast`
- `nix develop -c cargo test -p php_std shmop --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=shmop`
