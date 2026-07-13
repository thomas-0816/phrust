# igbinary PHPT coverage

## Scope

- Generated PHPTs cover `igbinary_serialize` and `igbinary_unserialize` for null, booleans, integers, floats, byte strings, packed arrays, mixed arrays, and repeated string-table references.
- The cache serializer smoke verifies Redis and Memcached accept `SERIALIZER_IGBINARY`; VM unit tests cover endpoint-backed Redis and Memcached igbinary payload encode/decode for structured values.

## Gaps

- Object serialization hooks, incomplete object restoration, reference identity, cyclic structures, and session serializer registration are not complete.
- Redis and Memcached JSON serializer constants are option-compatible but still fail closed for endpoint-backed payload I/O; this slice wires igbinary, MessagePack, and PHP serialize payload formats only.
- Upstream igbinary does not appear to register public PHP constants in the current source; no non-parity constants are added here.

## Gates

```bash
nix develop -c cargo test -p php_runtime igbinary --no-fail-fast
nix develop -c cargo test -p php_vm redis --no-fail-fast
nix develop -c cargo test -p php_vm memcached --no-fail-fast
nix develop -c cargo test -p php_std igbinary --no-fail-fast
nix develop -c just phpt-dev-module MODULE=igbinary
```
