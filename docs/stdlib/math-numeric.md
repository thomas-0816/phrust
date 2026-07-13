# Standard library Math and Numeric Functions

Reference target: PHP 8.5.7 (`php-8.5.7`).

The standard library implements the common standard math and numeric helpers used by
Composer-style bootstrap and framework code:

- `abs`, `min`, `max`
- `round`, `floor`, `ceil`
- `sqrt`, `pow`
- `intdiv`, `fmod`
- `is_finite`, `is_infinite`, `is_nan`
- `number_format`

## Compatibility Surface

The implementation reuses the runtime numeric coercion helpers so integer,
float, and numeric-string inputs follow the same conversion path as existing
casts and loose comparisons. `min` and `max` use the runtime PHP comparison
helper and preserve the selected input value rather than coercing the result.

`intdiv` and `fmod` reject division by zero with deterministic runtime value
errors. The focused runtime tests cover those errors directly because the VM
does not yet model PHP's exact `DivisionByZeroError` and `ValueError` class
surface for builtins.

`number_format` covers decimal precision plus custom decimal and thousands
separators. It is intentionally deterministic and byte-oriented.

## Float Notes

Float operations use Rust `f64` and therefore inherit host IEEE-754 behavior for
NaN, INF, overflow, and exact binary rounding. Differential fixtures normalize
ordinary float output and cover common finite values plus `sqrt(-1)` and
`1e309` for NaN/INF predicates.

The following PHP edge surfaces are tracked as
`STDLIB-GAP-MATH-FLOAT-EDGES`:

- all `PHP_ROUND_*` modes and exact tie behavior
- negative-zero formatting details
- overflow shape for large integer powers
- architecture-sensitive final digits outside the normalized fixture set

## Validation

The standard library is covered by:

- runtime unit tests in `php_runtime::builtins`
- registry metadata tests in `php_std`
- `tests/fixtures/stdlib/_harness/stdlib/math_numeric.php`
- known-gap fixture
  `tests/fixtures/stdlib/_harness/stdlib/math_numeric_float_edges.php`
- `just diff-stdlib`
- `just performance-tests`
- `just verify-stdlib`
