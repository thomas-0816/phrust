# igbinary PHPT coverage

## Implemented slice

- Registers the `igbinary` extension in `php_std` and the runtime builtin
  registry.
- Implements `igbinary_serialize` and `igbinary_unserialize`.
- Encodes PHP `null`, booleans, integers, floats, byte strings, packed arrays,
  and mixed arrays with integer/string keys.
- Decodes igbinary v1/v2 payloads for null, booleans, integers, floats,
  strings, string-table references, and arrays with PHP-compatible int/string
  key normalization.
- Matches the PECL-documented hex payload for `["first", true]`:
  `000000021402060011056669727374060105`.

## Known gaps

- Object serialization hooks, incomplete-object restoration, reference identity,
  and cyclic structures are not implemented yet.
- Session serializer registration is not implemented yet.
- Redis and Memcached serializer option integration is still pending.
- igbinary reference/object opcodes fail closed with a warning and `NULL`.
- The pinned php-src CLI used by this workspace does not load PECL igbinary, so
  the generated PHPT uses a standard extension-loaded skip contract for the
  reference side.

## Gates

- `nix develop -c cargo test -p php_runtime igbinary --no-fail-fast`
- `nix develop -c cargo test -p php_std igbinary --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=igbinary`
