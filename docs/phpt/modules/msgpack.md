# msgpack PHPT coverage

## Scope

- Generated PHPTs cover `msgpack_pack`, `msgpack_serialize`, `msgpack_unpack`, and `msgpack_unserialize` for null, booleans, integers, floats, byte strings, packed arrays, and mixed arrays.
- Class visibility for `MessagePack` and `MessagePackUnpacker` is covered.
- Global `MESSAGEPACK_OPT_PHPONLY`, `MESSAGEPACK_OPT_ASSOC`, and `MESSAGEPACK_OPT_FORCE_F32` constants are covered with values from the upstream extension.
- The cache serializer smoke verifies Redis and Memcached accept msgpack serializer options; VM unit tests cover endpoint-backed Redis and Memcached MessagePack payload encode/decode for structured values.

## Gaps

- Object serialization hooks, reference identity, cyclic structures, extension records, timestamp records, and `MessagePack`/`MessagePackUnpacker` object APIs are not complete.
- Redis and Memcached JSON serializer constants are option-compatible but still fail closed for endpoint-backed payload I/O; this slice wires MessagePack, igbinary, and PHP serialize payload formats only.

## Gates

```bash
nix develop -c cargo test -p php_runtime msgpack --no-fail-fast
nix develop -c cargo test -p php_vm redis --no-fail-fast
nix develop -c cargo test -p php_vm memcached --no-fail-fast
nix develop -c cargo test -p php_std msgpack --no-fail-fast
nix develop -c just phpt-dev-module MODULE=msgpack
```
