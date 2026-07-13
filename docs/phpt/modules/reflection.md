# reflection PHPT coverage

## Verified scope

- Reflection metadata for functions, parameters, classes, methods, properties,
  attributes, enums, and extensions.
- Aggregate selected reflection behavior is verified through 23 selected PHPT
  fixtures against the php-src oracle.
- App extension class/function/method owner metadata is covered for PDO, curl,
  zip, fileinfo, OpenSSL object classes, DOM, and intl.

## Known gaps

- Full reflection API parity is not claimed.
- Doc comments, source locations, exhaustive default-value formatting, and
  every modifier/type edge case remain limited to selected fixtures.
