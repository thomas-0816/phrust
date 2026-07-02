# Standard library Work item Standard Library Stabilization
Reference target: PHP 8.5.7 (`php-8.5.7`).

Work item reviewed the current Standard library standard-library differential reports
and stabilized the highest-value deviations that were small runtime fixes rather
than new subsystem work.

## Fixed Deviations

| Fixture | Previous gap | Stabilization |
| --- | --- | --- |
| `STDLIB_ARRAY_FLIP_WARNING` | `STDLIB-GAP-ARRAY-FLIP-WARNING-TEXT` | `array_flip` now skips unsupported values with PHP-style display warnings and keeps the valid flipped entries. |
| `STDLIB_ENCODING_INVALID_HEX` | `STDLIB-GAP-HEX2BIN-WARNING-TEXT` | `hex2bin` now distinguishes odd-length and non-hex input, returning `false` with PHP-style warnings. |
| `STDLIB_UNSERIALIZE_MALFORMED_WARNING` | `STDLIB-GAP-UNSERIALIZE-WARNING-TEXT` | malformed `unserialize` input now returns `false` with a PHP-style offset warning. |
| `STDLIB_SYMBOL_TRAIT_INTROSPECTION` | `STDLIB-GAP-SYMBOL-TRAIT-INTROSPECTION` | `trait_exists` and `get_declared_traits` now observe user traits through the VM symbol-introspection path. |
| `STDLIB_STRING_NUL_SOURCE_ESCAPE` | `STDLIB-GAP-SOURCE-NUL-ESCAPE` | PHP source `\0` escape decoding now matches the PHP 8.5.7 reference for the focused `strlen("a\0b")` fixture. |

## Reviewed And Kept As Known Gaps

These deviations remain explicit known gaps because fixing them requires parser,
frontend, lvalue/reference, stream-resource, or broad PHP parity work outside a
small standard-library stabilization slice.

| Fixture or PHPT | Gap | Reason |
| --- | --- | --- |
| `STDLIB_ARRAY_NATURAL_SORT_EDGE` | `STDLIB-GAP-NATURAL-SORT-EDGE-CASES` | Requires byte-parity with Zend natural comparison across leading-zero and locale-sensitive edges. |
| `STDLIB_ARRAY_WALK_BY_REF_MUTATION` | `STDLIB-GAP-ARRAY-WALK-BY-REF-MUTATION` | Requires callback argument slots to carry element references and propagate mutation through VM lvalues. |
| `STDLIB_FORMATTING_FPRINTF` | `STDLIB-GAP-FPRINTF-STREAM-RESOURCE` | Requires formatted output to write through PHP stream resources. |
| `STDLIB_MATH_NUMERIC_FLOAT_EDGES` | `STDLIB-GAP-MATH-FLOAT-EDGES` | Requires PHP rounding modes, negative-zero formatting, overflow, and architecture-sensitive float parity. |
| `ext/standard/tests/general_functions/var_dump_bools.phpt` | `STDLIB-GAP-EXTENSION-PHPT-PROMOTION` | Exposes encapsed string interpolation lowering before the PHPT can be promoted to runnable. |
| `ext/date/tests/date_default_timezone_set-1.phpt` | `STDLIB-GAP-EXTENSION-PHPT-PROMOTION` | Depends on PHPT INI and host-timezone behavior beyond the deterministic timezone MVP. |

## Validation

- `nix develop -c scripts/stdlib_diff.py --fixtures tests/fixtures/stdlib/_harness --out target/stdlib/diff-06-54-focused --file tests/fixtures/stdlib/_harness/stdlib/array_flip_warning.php --file tests/fixtures/stdlib/_harness/stdlib/encoding_invalid_hex.php --file tests/fixtures/stdlib/_harness/stdlib/unserialize_malformed.php`
- `nix develop -c just diff-stdlib`
- `nix develop -c just performance-tests`
