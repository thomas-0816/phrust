# msgpack PHPT coverage

## Implemented slice

- Registers the `msgpack` extension in `php_std` and the runtime builtin
  registry.
- Implements `msgpack_pack`/`msgpack_serialize` and
  `msgpack_unpack`/`msgpack_unserialize` aliases.
- Exposes `MessagePack` and `MessagePackUnpacker` class symbols for
  compatibility probes.
- Encodes PHP `null`, booleans, integers, floats, byte strings, packed arrays,
  and mixed arrays with integer/string keys.
- Decodes MessagePack nil, booleans, signed/unsigned integers in PHP integer
  range, floats, strings, binary payloads, arrays, and maps with PHP-compatible
  int/string key normalization.

## Known gaps

- Object serialization hooks, incomplete-object restoration, reference identity,
  and cyclic structures are not implemented yet.
- `MessagePack::OPT_PHPONLY`, `MessagePack::OPT_ASSOC`, and the object API
  methods are not implemented yet.
- Extension types, timestamp records, and unsigned integers larger than PHP's
  integer range are rejected rather than coerced.
- Redis and Memcached serializer option integration is still pending.
- The pinned php-src CLI used by this workspace does not load PECL msgpack, so
  the generated PHPT uses a standard extension-loaded skip contract for the
  reference side.

## Gates

- `nix develop -c cargo test -p php_runtime msgpack --no-fail-fast`
- `nix develop -c cargo test -p php_std msgpack --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=msgpack`
