# Language Gap 7C Program Composition Current Report

This report records the Prompt Pack 7C closeout for PHP 8.5.7 language-spec
gaps around program composition, callables, diagnostics, parser strings, and
language-visible standard-library support.

## Gap Closures

| Area | Closed evidence | Notes |
| --- | --- | --- |
| Parser strings and source maps | `nix develop -c just parser-diff` compared 67 parser fixtures with `allowed gaps=0` | No parser source-map behavior changed in this closeout; the existing string, heredoc, and nowdoc coverage stayed green. |
| Include/eval declarations | `fixtures/runtime_semantics/include_eval_autoload/include-conditional-function.php`, `fixtures/runtime_semantics/include_eval_autoload/eval-conditional-function.php` | Conditional include/eval function declarations now remain execution-time side effects and are callable only after control reaches the declaration. |
| Include warnings | `fixtures/runtime_semantics/includes/include-missing-warning.php`, `fixtures/runtime_semantics/include_eval_autoload/include-missing.php` | Missing `include` warnings now render PHP-compatible stdout, use PHP's default `include_path='.:'` display, continue execution, and suppress duplicate structured stderr when the warning is already visible. |
| Include/eval constant redeclarations | `fixtures/runtime_semantics/include_eval_autoload/include-redeclare-constant-fails.php`, `fixtures/runtime_semantics/include_eval_autoload/eval-redeclare-constant-fails.php` | Duplicate global `const` declarations now emit PHP 8.5-compatible warnings, preserve the first constant value, continue execution, and render include/eval source locations for the covered fixtures. |
| Conversion warnings | `fixtures/runtime_semantics/conversions/arithmetic-numeric-strings.php`, `fixtures/runtime_semantics/conversions/array-to-string.php` | Numeric-string arithmetic and array-to-string conversion warnings now render with PHP-compatible source file/line context while preserving continuation behavior and conversion results. |
| Non-numeric arithmetic TypeError output | `fixtures/runtime_semantics/conversions/non-numeric-arithmetic.php` | The selected non-numeric string arithmetic `TypeError` now renders PHP-compatible fatal stdout and stack text, suppresses duplicate structured stderr, and exits with PHP's fatal status. |
| String offset diagnostics | `fixtures/runtime_semantics/strings/string-offset-diagnostics.php` | Negative string-offset writes now emit PHP-compatible warnings and continue without changing the string; non-integer string offsets now throw catchable `TypeError` objects for the covered fixture. |
| Callable names | `fixtures/runtime_semantics/callables/namespaced-string-callable.php` | `__NAMESPACE__` now lowers with namespace context for top-level statements and function bodies, so namespaced string callables execute through the unified VM callable path. |
| Constant expressions | `fixtures/runtime_semantics/const_expr/cast-default.php` | Scalar cast parameter defaults such as `(int) "42"` now fold into IR constants and execute with PHP-compatible default values. |
| Type/runtime storage | `fixtures/runtime_semantics/types/static-property.php` | Static property storage and fixture-covered typed reads are implemented; by-reference and visibility edge cases stay tracked by narrower property/reference gaps. |
| Strict parameter TypeError output | `fixtures/runtime_semantics/types/param-strict-rejects-string.php` | The selected strict parameter `TypeError` now renders PHP-compatible fatal stdout and stack text, suppresses duplicate structured stderr, and exits with PHP's fatal status. |
| Uninitialized property fatal output | `fixtures/runtime_semantics/types/uninitialized-property-fail.php` | Selected uninitialized typed-property reads now render PHP-compatible fatal stdout text and stack wording, suppress duplicate structured stderr, and exit with PHP's fatal status. |
| Union parameter TypeError output | `fixtures/runtime_semantics/types/union-param-fail.php` | The selected union parameter `TypeError` now renders PHP-compatible scalar union ordering, fatal stdout and stack text, suppresses duplicate structured stderr, and exits with PHP's fatal status. |
| Program-composition autoload | `fixtures/runtime_semantics/include_eval_autoload/autoload-relation-cache.php` | Focused relation-cache autoload lookup remains green after the gap promotion. |
| Language-visible stdlib | `tests/fixtures/stdlib/_harness/stdlib/string_nul_source_escape.php` | PHP source `\0` escape decoding matches PHP 8.5.7 for the focused `strlen("a\0b")` fixture. |

The runtime gap report now reads:

```text
total=97 open=69 implemented=28
```

## PHPT Module Counts

| Module | Starting inventory | Closeout result |
| --- | ---: | ---: |
| `zend.functions` | reference 29 / target 29, 0 non-green | reference 29 / target 29, 0 non-green |
| `zend.basic` | reference 10 / target 10, 0 non-green | reference 10 / target 10, 0 non-green |
| `diagnostics.output` | reference 6 / target 6, 0 non-green | reference 6 / target 6, 0 non-green |
| `closure.stdlib` | reference 49 / target 49, 0 non-green | reference 49 / target 49, 0 non-green |

## Targeted Runtime Semantics

The focused Prompt 7C diff across include/eval, includes, callables, types,
errors, strings, const expressions, and conversions is:

```text
total=103 pass=99 fail=0 skip=0 known_gap=4
```

Remaining known gaps in that focused matrix:

| Area | Gap IDs |
| --- | --- |
| Constant expressions | `E_PHP_RUNTIME_CONST_EXPR_MATRIX` |
| Type diagnostic text and references | `E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE` |

These remain explicit because they require callable/closure/object
constant-expression materialization or reference lvalue semantics beyond the
focused fixture promotions in this closeout.

## Validation

| Gate | Result |
| --- | --- |
| `nix develop -c cargo fmt --check` | PASS |
| `nix develop -c cargo test -p php_lexer` | PASS |
| `nix develop -c cargo test -p php_syntax` | PASS |
| `nix develop -c cargo test -p php_semantics` | PASS |
| `nix develop -c cargo test -p php_ir` | PASS |
| `nix develop -c cargo test -p php_ir parameter_default_expression_matrix_lowers_to_ir_constants` | PASS |
| `nix develop -c cargo test -p php_vm` | PASS |
| `nix develop -c cargo test -p php_runtime errors` | PASS |
| `nix develop -c just parser-diff` | PASS, 67 fixtures, allowed gaps 0 |
| `nix develop -c just runtime-semantics-fixtures` | PASS |
| `nix develop -c just runtime-known-gaps` | PASS, 110 manifest entries |
| `nix develop -c just runtime-gap-report` | PASS, total 97 / open 69 / implemented 28 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/const_expr/cast-default.php --out target/runtime-semantics/language-gap-7c-cast-default-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/include_eval_autoload/include-redeclare-constant-fails.php --file fixtures/runtime_semantics/include_eval_autoload/eval-redeclare-constant-fails.php --out target/runtime-semantics/language-gap-7c-constant-redecl-promoted` | PASS, total 2 / pass 2 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/types/param-strict-rejects-string.php --out target/runtime-semantics/language-gap-7c-typeerror-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/types/uninitialized-property-fail.php --out target/runtime-semantics/language-gap-7c-uninitialized-property-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/types/union-param-fail.php --out target/runtime-semantics/language-gap-7c-union-typeerror-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/conversions/non-numeric-arithmetic.php --out target/runtime-semantics/language-gap-7c-non-numeric-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --file fixtures/runtime_semantics/strings/string-offset-diagnostics.php --out target/runtime-semantics/language-gap-7c-string-offset-promoted` | PASS, total 1 / pass 1 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php nix develop -c scripts/runtime_semantics_diff.py --category include_eval_autoload --category includes --category callables --category types --category errors --category strings --category const_expr --category conversions --out target/runtime-semantics/language-gap-7c-current` | PASS, total 103 / pass 99 / known_gap 4 |
| `nix develop -c just verify-runtime` | PASS |
| `nix develop -c just verify-phpt` | PASS, with 6 host-generated php-src source-integrity entries skipped on this platform |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=zend.functions` | PASS, reference 29 / target 29 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=zend.basic` | PASS, reference 10 / target 10 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=diagnostics.output` | PASS, reference 6 / target 6 |
| `REFERENCE_PHP=$PWD/third_party/php-src/sapi/cli/php PHP_SRC_DIR=$PWD/third_party/php-src nix develop -c just phpt-dev-module MODULE=closure.stdlib` | PASS, reference 49 / target 49 |
