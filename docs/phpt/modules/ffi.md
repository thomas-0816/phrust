# FFI PHPT coverage

## Implemented slice

- Registers the `ffi` extension using generated arginfo metadata for the public
  class surface.
- Exposes `FFI`, `FFI\CData`, `FFI\CType`, `FFI\Exception`, and
  `FFI\ParserException` as internal classes.
- Covers reflection metadata for the disabled-by-default `FFI` class and static
  helper methods.
- Adds a VM static-method hook that fails closed for unsafe FFI calls with a
  deterministic unsupported diagnostic instead of attempting `libffi` or
  `dlopen` behavior.
- Covers `FFI::cdef()` in the generated PHPT suite as a disabled-by-default
  fatal runtime diagnostic.
- Exposes `ffi.enable=preload` and `ffi.preload=` through the request-local INI
  registry, `ini_get`, `get_cfg_var`, and `ini_get_all('ffi', ...)`, while
  keeping runtime `ini_set('ffi.enable', ...)` read-only.

## Known gaps

- Unsafe FFI execution is not implemented. `FFI::cdef`, `FFI::load`,
  allocation, casts, type parsing, scope loading, CData/CType object behavior,
  and memory helpers all require an explicit future capability gate.
- FFI preload execution and scope loading are not implemented; the INI entries
  are visibility and policy metadata only.
- Server-mode restrictions beyond read-only default-off INI metadata and
  fail-closed runtime dispatch are not implemented.
- Platform ABI constants and exact `FFI\Exception` / `FFI\ParserException`
  throw-site parity are out of this slice.
- The local php-src oracle CLI currently does not load `ext/ffi`; reference
  promotion is therefore limited to target-side generated fixtures until an FFI
  oracle is available.

## Gates

- `nix develop -c cargo test -p php_std ffi --no-fail-fast`
- `nix develop -c cargo test -p php_vm ffi --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=ffi`
