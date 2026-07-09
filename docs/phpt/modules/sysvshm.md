# sysvshm PHPT coverage

Current focused coverage:

- `sysvshm` extension visibility, `SysvSharedMemory` class visibility, and
  function registration.
- Request-local shared variable segments from `shm_attach()`.
- Variable key behavior for `shm_has_var()`, `shm_put_var()`,
  `shm_get_var()`, and `shm_remove_var()`.
- `shm_attach()` argument validation, object display, storage capacity warnings,
  `shm_detach()`, `shm_remove()`, serialization callback side effects,
  userland `__serialize()` exception propagation, and destroyed-handle `Error`
  behavior.

Selected coverage now includes the generated contract row plus these upstream
PHP 8.5.7 rows:

- `ext/sysvshm/tests/001.phpt`
- `ext/sysvshm/tests/002.phpt`
- `ext/sysvshm/tests/003.phpt`
- `ext/sysvshm/tests/004.phpt`
- `ext/sysvshm/tests/005.phpt`
- `ext/sysvshm/tests/006.phpt`
- `ext/sysvshm/tests/007.phpt`
- `ext/sysvshm/tests/gh16591.phpt`
- `ext/sysvshm/tests/serialize_exception.phpt`
- `ext/sysvshm/tests/shutdown_crash_0.phpt`

This slice uses deterministic request-local storage for isolated tests.
The current full upstream target sweep is 10 PASS / 0 FAIL / 2 SKIP. Host-local
skips are `bug72858.phpt` and `shm_get_var_leak.phpt`.

Focused gate:

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
nix develop -c just phpt-dev-module MODULE=sysvshm
```
