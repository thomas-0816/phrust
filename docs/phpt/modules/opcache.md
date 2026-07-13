# opcache PHPT coverage

## Verified scope

- `opcache` extension visibility.
- VM compiler-backed, request-local `opcache_compile_file()` cache tracking for
  files that successfully compile.
- `opcache_is_script_cached()` probes for request-local cached scripts.
- `opcache_get_status()` facade data, including enabled state and cached-script
  statistics selected by the fixture.
- `opcache_get_configuration()` facade data for selected directives and version
  metadata.
- `opcache_invalidate()` and `opcache_reset()` request-local cache mutation.
- Invalid PHP source is not recorded as cached by the facade.

## Known gaps

- This is not a Zend Opcache replacement.
- Preloading, persistent file cache semantics, optimizer passes, and JIT are
  outside the selected manifest.
- Cross-request cache sharing is not claimed by the request-local facade.
- The standalone runtime facade cannot compile PHP source; compile-file success
  is provided by the VM dispatch hook.
- Server warm-cache behavior remains future promotion work beyond the selected
  PHPT fixture.
