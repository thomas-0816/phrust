# standard.arrays PHPT coverage

## Verified scope

- Selected high-frequency array helpers including count, key/value extraction,
  merge, slice, splice, column, search, uniqueness, range/fill/pad, stack
  operations, chunking, sorting, filtering, mapping, reducing, and traversal
  helpers.
- PHP-visible behavior is verified through 35 selected PHPT fixtures against
  the php-src oracle.

## Known gaps

- Full `ext/standard` array corpus parity is not claimed.
- Deep callback, warning-text, reference, sorting, and mutation edge cases
  remain future promotion work outside the selected manifest.
