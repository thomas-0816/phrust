# core PHPT coverage

## Implemented slice

- Registers the `core` extension through `php_std` and exposes the pinned PHP
  version constants for PHP 8.5.7.
- Covers selected PHP error constants, `extension_loaded`, `php_sapi_name`,
  `ini_get`, `ini_set`, `ini_get_all`, and `get_cfg_var` symbol visibility.
- Verifies `memory_limit` and `serialize_precision` defaults, per-request
  `ini_set("memory_limit", ...)`, and unknown INI-name failure behavior.
- Verifies `get_defined_constants(true)` and `get_defined_functions()` expose
  selected core symbols.
- Verifies selected core throwable, exception, and error class/interface
  symbols are visible.

## Known gaps

- The selected fixture does not prove the full php-src Zend/core PHPT corpus.
- `get_cfg_var("memory_limit")` is not claimed as covered; the oracle returns
  `false` for this selected runtime default while phrust currently exposes the
  active runtime value.
- INI access-mode enforcement and exact diagnostics remain covered by narrower
  Zend and runtime follow-up work.
- Exception object method behavior is not proven by this module fixture.

## Gates

- `nix develop -c cargo test -p php_runtime core --no-fail-fast`
- `nix develop -c cargo test -p php_std core --no-fail-fast`
- `REFERENCE_PHP=$REFERENCE_PHP PHP_SRC_DIR=$PHP_SRC_DIR PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=core`
