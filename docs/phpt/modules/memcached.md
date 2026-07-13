# memcached PHPT coverage

## Scope

- Generated PHPTs cover the endpoint-backed `Memcached` client with CI-safe
  connection-failure behavior when no daemon is configured.
- The client uses a small isolated text-protocol client rather than an
  in-memory fake.
- Covered APIs include construction with optional persistent ID, `addServer`,
  `addServers`, `getServerList`, `get`, `set`, `add`, `replace`, `delete`,
  `getMulti`, `setMulti`, `increment`, `decrement`, options, and result
  codes/messages.
- Live endpoint coverage is default-off behind `PHRUST_MEMCACHED_LIVE_ENDPOINT`.
- Covered constants include selected result code, serializer, compression, prefix, and multi-get option constants used by high-level cache integrations.

## Gaps

- Persistent sockets and persistent ID pooling.
- Binary protocol, TLS, SASL, and session handlers.
- Serializer wire-format compatibility, compression, and full daemon
  result-code transition parity.
- Non-prompt compatibility helpers such as append/prepend/CAS/touch/flush
  remain deterministic placeholders unless endpoint protocol coverage is
  promoted.

## Gate

```bash
nix develop -c cargo test -p php_runtime memcached -- --nocapture
nix develop -c cargo test -p php_vm memcached -- --nocapture
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=memcached
```

Latest focused result:

- `cargo test -p php_runtime memcached`: PASS, 0 matching runtime tests.
- `cargo test -p php_vm memcached`: PASS, 1 VM test.
- `just phpt-dev-module MODULE=memcached`: reference SKIP 3; target PASS 2,
  SKIP 1; php-src manifest integrity verified 24468 entries with 7
  host-generated skips.
