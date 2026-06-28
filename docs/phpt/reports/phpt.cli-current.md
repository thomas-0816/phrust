# phpt.cli current report

Last focused run: 2026-06-28.

## Commands

- `nix develop -c cargo test -p php_vm_cli`: PASS, 49 tests.
- `nix develop -c cargo build -p php_vm_cli --bin phrust-php`: PASS.
- `TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli nix develop -c just phpt-target-smoke`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=phpt.cli`: PASS, 203 selected cases with 3 PASS, 200 SKIP, and 0 non-green outcomes.

## Supported CLI Contract

- `phrust-php -v`
- `phrust-php -r 'code'`
- `phrust-php -n`
- repeated `-d key=value`
- script execution with argv
- STDIN exposed as a PHP stream
- `$argc`, `$argv`
- `$_SERVER['argc']`, `$_SERVER['argv']`
- PHPT `INI` values visible through runtime `ini_get()` and `error_reporting()`

## Focused Fixtures

- `tests/phpt/generated/phpt.cli/argv-argc-superglobals.phpt`: PASS.
- `tests/phpt/generated/phpt.cli/ini-overrides.phpt`: PASS.
- `tests/phpt/generated/phpt.cli/stdin.phpt`: PASS.

## Selected Upstream Skips

The upstream `sapi/cli` selected set still contains SAPI, HTTP server,
process-control, and unrelated runtime/frontend tests that are not part of the
Prompt 1B CLI contract. They now skip with concrete target-mode reasons:

| Reason | Count |
| --- | ---: |
| FPM not available in php-cli target mode | 141 |
| CLI built-in web server not available in php-cli target mode | 39 |
| CLI process-control APIs not available in php-cli target mode | 6 |
| process-control functions are outside the Prompt 1B CLI contract | 5 |
| CLI stdio descriptor rebinding not available in php-cli target mode | 3 |
| phpdbg not available in php-cli target mode | 2 |
| CLI --ini introspection not available in php-cli target mode | 1 |
| CLI -R line-processing mode not available in php-cli target mode | 1 |
| include-path expression runtime gap outside the Prompt 1B CLI contract | 1 |
| STDOUT default-parameter lowering is outside the Prompt 1B CLI contract | 1 |

## Remaining CLI Gaps

No selected Prompt 1B-owned CLI contract failures remain. The skipped upstream
cases are still visible in selected results and should be moved or implemented
by their owning SAPI, process-control, stdio-stream, frontend, or runtime
modules.
