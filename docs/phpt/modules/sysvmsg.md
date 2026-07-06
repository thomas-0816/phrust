# sysvmsg PHPT coverage

## Implemented slice

- Registers the PHP 8.5 `sysvmsg` function, constant, and
  `SysvMessageQueue` class surface in `php_std` and the runtime builtin
  registry.
- Implements `msg_get_queue`, `msg_send`, `msg_receive`,
  `msg_remove_queue`, `msg_stat_queue`, `msg_set_queue`, and
  `msg_queue_exists`.
- Uses a deterministic request-local queue backend. Keyed queues share state
  inside one request and removal clears pending messages.
- Preserves PHP serialized payload behavior for `$serialize=true`, raw scalar
  payload behavior for `$serialize=false`, and by-reference receive outputs for
  message type, message value, and error code.

## Known gaps

- The backend is not cross-process System V IPC and does not allocate host
  kernel message queues.
- Permission checks, kernel queue IDs, blocking waits, and platform-specific
  errno warning text are bounded to deterministic request-local behavior.
- The pinned php-src CLI used by this workspace does not load ext/sysvmsg, so
  upstream reference PHPTs may skip on the reference side while the generated
  target fixture exercises the Phrust implementation.

## Gates

- `nix develop -c cargo test -p php_runtime sysv --no-fail-fast`
- `nix develop -c cargo test -p php_std sysv --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=sysvmsg`
