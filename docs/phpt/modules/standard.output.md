# standard.output PHPT coverage

## Verified scope

- Output buffering basics.
- Nested output-buffer behavior selected by the module manifest.
- PHP-visible behavior is verified through 11 selected PHPT fixtures against
  the php-src oracle.

## Known gaps

- Full output-handler lifecycle, callback, flush, compression, and shutdown
  ordering parity is not claimed.
- Additional web-SAPI interactions should be promoted as selected fixtures
  before being claimed.
