# standard.serialization PHPT coverage

## Verified scope

- Selected `serialize()` and `unserialize()` behavior.
- Value persistence and selected upstream scalar, option, and edge-case
  serialization regressions.
- PHP-visible behavior is verified through 23 selected PHPT fixtures against
  the php-src oracle.

## Known gaps

- Full object serialization, magic method ordering, references, incomplete
  classes, and malformed payload diagnostics remain outside the selected
  manifest unless covered by narrower fixtures.
