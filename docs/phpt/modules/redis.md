# redis PHPT coverage

## Scope

- Generated PHPTs cover the endpoint-backed `Redis` client with CI-safe
  connection-failure behavior when no daemon is configured.
- The client uses the Rust `redis` crate for configured endpoints.
- Covered APIs include extension/class introspection, construction,
  `connect`/`pconnect`, `auth`, `select`, `close`, `ping`, `isConnected`,
  key/value operations, counters, `mget`/`mset`, hashes, lists, `ttl`,
  `expire`, `persist`, and deletion.
- Live endpoint coverage is default-off behind `PHRUST_REDIS_LIVE_ENDPOINT`.
- Covered constants include phpredis option, serializer, compression, scan, and `ATOMIC`/`MULTI`/`PIPELINE` mode constants.

## Gaps

- Persistent socket pooling beyond pconnect-compatible connect behavior.
- Cluster, sentinel, pub/sub, Lua `eval`, streams, blocking commands,
  serializer wire format, and compression backend.
- Non-prompt compatibility helpers such as scan/sets/sorted sets remain
  deterministic placeholders, not endpoint-backed protocol coverage.

## Gate

```bash
nix develop -c cargo test -p php_runtime redis -- --nocapture
nix develop -c cargo test -p php_vm redis -- --nocapture
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=redis
```

Latest focused result:

- `cargo test -p php_runtime redis`: PASS, 0 matching runtime tests.
- `cargo test -p php_vm redis`: PASS, 1 VM test.
- `just phpt-dev-module MODULE=redis`: reference SKIP 3; target PASS 2,
  SKIP 1; php-src manifest integrity verified 24468 entries with 7
  host-generated skips.
