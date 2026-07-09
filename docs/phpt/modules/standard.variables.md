# standard.variables PHPT coverage

## Verified scope

- Selected variable inspection and conversion builtins.
- Debug output for scalar, string, and array values.
- The current selected gate verifies 32 PASS and 1 SKIP target outcomes from
  33 selected fixtures.

## Known gaps

- Full `var_dump()`, `debug_zval_dump()`, object/resource formatting, reference
  diagnostics, and platform-sensitive output parity remain future work outside
  the selected manifest.
