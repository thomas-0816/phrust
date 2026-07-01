# Runtime Gap Closure: References, Arrays, Foreach

## Closed behavior

- `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` was narrowed: resolved
  static properties can now be passed to userland by-reference parameters
  through the existing `Slot`/`ReferenceCell`/`Lvalue` storage model.
- No broad runtime gap row was reclassified as `implemented`; the remaining
  property-hook/magic-property aliases and temporary by-reference foreach
  sources are still explicit gaps.

## Fixture movement

- Moved
  `fixtures/runtime_semantics/functions/param-by-ref-static-property-known-gap.php`
  to `fixtures/runtime_semantics/functions/param-by-ref-static-property.php`.
- The moved fixture now has `runtime-semantics: category=functions expect=pass`
  and passes against PHP 8.5.7 and the Rust VM.

## PHPT movement

- No PHPT was promoted in this pass. The required selected module inventories
  stayed green against the pinned PHP 8.5.7 oracle.

## Remaining explicit gaps

- `E_PHP_RUNTIME_UNSUPPORTED_REFERENCE_SEMANTICS` still covers references that
  require property-hook or magic-property lvalues.
- `E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH` still covers by-reference foreach over
  temporary sources. Those sources are not storage locations and must not be
  silently converted to by-value iteration.
- `E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX` still covers `global $$name`, direct
  `$GLOBALS[]` fatal/proxy edge cases, and the broader SAPI interaction matrix.
- Array key conversion, numeric-string warning channel, full `var_dump`
  formatting, generator by-reference yield, and Traversable unpack remain
  tracked by their existing catalog rows.

## Validation

- `nix develop -c just runtime-gap-report`: PASS
- `nix develop -c just runtime-known-gaps`: PASS
- `nix develop -c just runtime-semantics-fixtures`: PASS
- `nix develop -c just runtime-fixtures`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=arrays.references`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=zend.basic`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=standard.variables`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=diagnostics.output`: PASS
- `nix develop -c cargo fmt --check`: PASS
- `nix develop -c cargo test -p php_runtime reference`: PASS
- `nix develop -c cargo test -p php_runtime array`: PASS
- `nix develop -c cargo test -p php_vm references`: PASS
- `nix develop -c cargo test -p php_vm foreach`: PASS
- `nix develop -c cargo test -p php_vm static_property_by_ -- --nocapture`: PASS
- `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/functions/param-by-ref-static-property.php --out target/runtime-semantics/static-property-by-ref`: PASS
- `nix develop -c just verify-runtime`: PASS
