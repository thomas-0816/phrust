# memcached PHPT coverage

## Scope

- Generated PHPTs cover the deterministic in-process `Memcached` fake backend used for CI-safe cache compatibility.
- Covered APIs include construction with optional persistent ID, `addServer`, `addServers`, `getServerList`, `get`, `set`, `add`, `replace`, `delete`, `getMulti`, `setMulti`, `deleteMulti`, `increment`, `decrement`, `touch`, `flush`, options, result codes/messages, `append`, `prepend`, CAS-shaped writes, and empty `getStats`/`getVersion` placeholders.
- Covered constants include selected result code, serializer, compression, prefix, and multi-get option constants used by high-level cache integrations.

## Gaps

- No external Memcached daemon protocol, persistent socket pool, binary protocol, TLS, SASL, session handler, serializer wire format, compression backend, or real TTL expiry is implemented in this deterministic fake backend.
- Live daemon transition parity tests remain opt-in future work.

## Gate

```bash
nix develop -c just phpt-dev-module MODULE=memcached
```
