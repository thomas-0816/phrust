# redis PHPT coverage

## Scope

- Generated PHPTs cover the deterministic in-process `Redis` fake backend used for CI-safe cache compatibility.
- Covered APIs include extension/class introspection, construction, `connect`/`pconnect`, `auth`, `select`, `close`, `ping`, `isConnected`, key/value operations, counters, `mget`/`mset`, hashes, lists, sets, sorted sets, `ttl`, `expire`, `persist`, deletion, `scan`, options, and transaction/pipeline mode placeholders.
- Covered constants include phpredis option, serializer, compression, scan, and `ATOMIC`/`MULTI`/`PIPELINE` mode constants.

## Gaps

- No external Redis TCP protocol, connection pooling, cluster, sentinel, pub/sub, Lua `eval`, streams, blocking commands, serializer wire format, or compression backend is implemented in this deterministic fake backend.
- Time-based expiration is bounded to return Redis-shaped `ttl`/`expire`/`persist` results without a wall-clock expiry engine.
- Live Redis integration tests remain opt-in future work.

## Gate

```bash
nix develop -c just phpt-dev-module MODULE=redis
```
