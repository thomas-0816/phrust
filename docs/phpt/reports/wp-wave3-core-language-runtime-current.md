# WP Wave 3 Core Language Runtime

This report tracks the `wp.core-language` gate for WordPress-like bootstrap
runtime semantics. The selected fixtures are generated, reference-oriented
contracts derived from Reference PHP 8.5.7 behavior.

## Selected PHPTs

| Fixture | Behavior |
| --- | --- |
| `tests/phpt/generated/wp.core-language/dynamic-function-call.phpt` | variable function dispatch |
| `tests/phpt/generated/wp.core-language/dynamic-method-call.phpt` | object method selected at runtime |
| `tests/phpt/generated/wp.core-language/dynamic-static-method-call.phpt` | static method selected at runtime |
| `tests/phpt/generated/wp.core-language/dynamic-class-instantiation.phpt` | `new $class(...)` |
| `tests/phpt/generated/wp.core-language/callable-array-invokable.phpt` | array callables and invokable objects |
| `tests/phpt/generated/wp.core-language/dynamic-property-access.phpt` | `$object->$property` read/write, dynamic `isset`/`empty`, and initialized dynamic-property dimension assignment |
| `tests/phpt/generated/wp.core-language/dynamic-property-negative.phpt` | non-object dynamic-property warnings and private-property access errors |
| `tests/phpt/generated/wp.core-language/nullsafe-property-method.phpt` | nullsafe method/property chain |
| `tests/phpt/generated/wp.core-language/named-arguments.phpt` | user-function named arguments |
| `tests/phpt/generated/wp.core-language/named-arguments-method-builtin.phpt` | method and selected builtin named arguments |
| `tests/phpt/generated/wp.core-language/named-argument-errors.phpt` | duplicate named-argument error mapping |
| `tests/phpt/generated/wp.core-language/argument-unpacking.phpt` | call argument unpacking |
| `tests/phpt/generated/wp.core-language/array-spread-unpack.phpt` | array spread/unpack with integer and string keys |
| `tests/phpt/generated/wp.core-language/destructor-local-scope.phpt` | destructor at local-scope release |
| `tests/phpt/generated/wp.core-language/destructor-after-script-end.phpt` | destructor ordering at request shutdown |
| `tests/phpt/generated/wp.core-language/shutdown-function-order.phpt` | shutdown callback order and captured globals |
| `tests/phpt/generated/wp.core-language/custom-error-handler.phpt` | custom warning handler and restore |
| `tests/phpt/generated/wp.core-language/error-restore-and-last.phpt` | `restore_error_handler()` and `error_get_last()` |
| `tests/phpt/generated/wp.core-language/exception-finally-edge.phpt` | finally before catch dispatch |

## Current Gate

```bash
REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php \
PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src \
PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 PHPT_DISABLE_REFERENCE_REUSE=1 \
nix --option eval-cache false develop -c just phpt-dev-build phpt-dev-module MODULE=wp.core-language
```

Result: reference 19 PASS, target 19 PASS, 0 non-green outcomes.

## Implementation Summary

- Added IR/runtime support for array spread insertion with PHP-style integer-key
  append and string-key overwrite semantics.
- Lowered dynamic object/static call forms into callable dispatch, including the
  HIR property-fetch carrier shape used by `$object->$method(...)`.
- Extended dynamic property, initialized dynamic-property dimension assignment,
  dynamic-property `isset`/`empty`, non-object dynamic-property warnings,
  inaccessible dynamic-property errors, nullsafe method/property, named
  argument, unpacking, shutdown function, destructor, custom error handler, and
  `error_get_last()` runtime behavior.
- Updated generated `wp.core-language` manifests from 11 initial selected
  fixtures to 19 closeout fixtures.
- Fixed hardening-lint failures found by `verify-runtime`.

## Validation

| Gate | Result |
| --- | --- |
| `cargo check -p php_ir -p php_optimizer -p php_vm -p php_semantics` | PASS |
| `cargo test -p php_ir` | PASS, 63 unit + 7 snapshot tests |
| `cargo test -p php_runtime` | PASS, 257 tests |
| `cargo test -p php_vm` | PASS, 450 tests |
| `cargo test -p php_std` | PASS, 59 lib tests + 1 bin test |
| `just phpt-dev-module MODULE=wp.core-language` | PASS, reference 19 / target 19 |
| `just phpt-dev-module MODULE=zend.functions` | PASS, reference 29 / target 29 |
| `just phpt-dev-module MODULE=arrays.references` | PASS, reference 6 / target 6 |
| `just phpt-dev-module MODULE=diagnostics.output` | PASS, reference 6 / target 6 |
| `just verify-runtime` | PASS |
| `just verify-phpt` | PASS |

## Neighbor Gate Notes

`just phpt-dev-module MODULE=objects.classes` was also run as a neighboring
gate. Reference passed 200/200; target passed 128/200 with 72 non-green
outcomes. The failures cluster in broader class-runtime surfaces outside this
slice, including fatal output formatting, static-property reference gaps,
catch-type gaps, eval declaration merging, and autoload/reflection edges.
Representative stable gap IDs in the run include
`E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE`,
`E_PHP_IR_UNSUPPORTED_CATCH_TYPE`, `E_PHP_VM_EVAL_DECLARATION_GAP`, and
`E_PHP_VM_UNKNOWN_CLASS`.

## Remaining Scope

No remaining `wp.core-language` fixture failures are known after this run.
Broader `objects.classes` parity remains outside this prompt slice and is
tracked by the owning class/runtime gap areas.
