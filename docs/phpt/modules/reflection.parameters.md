# reflection.parameters PHPT coverage

## Verified scope

- `ReflectionParameter` metadata from generated arginfo and IR.
- Position and variadic flags selected by the module manifest.

## Known gaps

- Full default-value, promoted-property, by-reference, union/intersection type,
  attribute, and callable edge cases remain future work.
