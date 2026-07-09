# standard.strings PHPT coverage

## Verified scope

- Selected high-frequency string helpers including length, substring, position,
  prefix/suffix/contains, case conversion, trimming, splitting/joining,
  formatting, replacement, tokenization, comparison, quoting, and escaping
  helpers.
- Hex-nibble `pack()`/`unpack()` formats `h` and `H`, including `*` counts,
  odd-nibble zero padding, and unpack cursor advancement.
- PHP-visible behavior is verified through 38 selected PHPT fixtures against
  the php-src oracle.

## Known gaps

- Full standard string PHPT parity is not claimed.
- Locale-specific behavior, exhaustive warning text, binary-string edge cases,
  and the full formatting matrix remain future selected-fixture work.
