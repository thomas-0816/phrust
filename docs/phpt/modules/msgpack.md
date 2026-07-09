# msgpack PHPT coverage

## Scope

- Generated PHPTs cover `msgpack_pack`, `msgpack_serialize`, `msgpack_unpack`, and `msgpack_unserialize` for null, booleans, integers, floats, byte strings, packed arrays, and mixed arrays.
- Class visibility for `MessagePack` and `MessagePackUnpacker` is covered.
- Global `MESSAGEPACK_OPT_PHPONLY`, `MESSAGEPACK_OPT_ASSOC`, and `MESSAGEPACK_OPT_FORCE_F32` constants are covered with values from the upstream extension.
- The cache serializer smoke verifies Redis and Memcached accept msgpack serializer options through their request-local fake backend option APIs.

## Gaps

- Object serialization hooks, reference identity, cyclic structures, extension records, timestamp records, and `MessagePack`/`MessagePackUnpacker` object APIs are not complete.
- Redis and Memcached serializer wire-format integration is not implemented; the current cache smoke proves option compatibility and PHP value storage only.

## Gates

```bash
nix develop -c cargo test -p php_runtime msgpack --no-fail-fast
nix develop -c cargo test -p php_std msgpack --no-fail-fast
nix develop -c just phpt-dev-module MODULE=msgpack
```
