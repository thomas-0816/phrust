# diagnostics.output current report

Last focused run: 2026-06-28.

## Commands

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=diagnostics.output`: PASS, 6 PASS and 0 non-green.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=zend.basic`: PASS, 10 PASS and 0 non-green.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src TARGET_PHP=target/debug/phrust-php PHPT_TARGET_MODE=php-cli PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=operators.conversions`: PASS, 4 PASS and 0 non-green.

## Covered Diagnostic Behavior

- undefined variable warning formatting and continuation
- array-to-string warning formatting and continuation
- missing include warning formatting and continuation
- invalid operand `TypeError` rendering through the fatal diagnostic channel
- builtin arity failures as catchable `ArgumentCountError`
- builtin type failures as catchable `TypeError`
- `display_errors` and `error_reporting` plumbing through shared runtime diagnostic options

## Output/Diagnostic Cluster

The current selected diagnostics gate has no non-green outcomes. Related
`zend.basic` and `operators.conversions` selected gates also have no non-green
outcomes. The missing-include fixture now covers the PHP two-warning include
failure shape in addition to the pre-existing warning and fatal diagnostic
cases.

## Remaining Wording Gaps

Full PHP wording parity remains broader than this selected gate. Unsupported
extensions, advanced runtime features, process-control behavior, and feature
modules that emit their own diagnostics remain tracked by their owning modules.
