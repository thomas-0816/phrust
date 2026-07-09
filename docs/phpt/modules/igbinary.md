# igbinary PHPT coverage

## Scope

- Generated PHPTs cover `igbinary_serialize` and `igbinary_unserialize` for null, booleans, integers, floats, byte strings, packed arrays, mixed arrays, and repeated string-table references.
- The cache serializer smoke verifies Redis and Memcached accept `SERIALIZER_IGBINARY` through their request-local option APIs while storing and retrieving PHP values through the deterministic fake backends.

## Gaps

- Object serialization hooks, incomplete object restoration, reference identity, cyclic structures, and session serializer registration are not complete.
- Redis and Memcached serializer wire-format integration is not implemented; the current cache smoke proves option compatibility and PHP value storage only.
- Upstream igbinary does not appear to register public PHP constants in the current source; no non-parity constants are added here.

## Gates

```bash
nix develop -c cargo test -p php_runtime igbinary --no-fail-fast
nix develop -c cargo test -p php_std igbinary --no-fail-fast
nix develop -c just phpt-dev-module MODULE=igbinary
```
