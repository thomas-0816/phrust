# reflection PHPT coverage

## Verified scope

- Reflection metadata for functions, parameters, classes, methods, properties,
  attributes, enums, and extensions.
- Aggregate selected reflection behavior is verified through 22 selected PHPT
  fixtures against the php-src oracle.

## Known gaps

- Full reflection API parity is not claimed.
- Doc comments, source locations, exhaustive default-value formatting, and
  every modifier/type edge case remain limited to selected fixtures.
