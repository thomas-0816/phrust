# intl PHPT coverage

## Verified scope

- `intl` extension visibility.
- Common bounded symbol visibility for `intl_get_error_code()`,
  `grapheme_strlen()`, `normalizer_normalize()`, `Locale`,
  `NumberFormatter`, `Collator`, `IntlChar`, and `Normalizer`.
- `Normalizer::isNormalized()` and `Normalizer::normalize()` for the selected
  NFC-compatible ASCII/UTF-8 fallback cases.
- Procedural `normalizer_*` aliases for the same bounded normalization slice.
- `grapheme_strlen()` and `grapheme_substr()` over valid UTF-8 character
  sequences used by framework fallback paths.
- `transliterator_transliterate("Latin-ASCII", ...)` for the selected Latin
  accent-removal fixture.
- `intl_get_error_code()` returning the no-error state for supported bounded
  helpers.
- ICU version/data constants selected by the module manifest.

## Known gaps

- No ICU-backed locale data layer is connected.
- Collation parity, formatter behavior, MessageFormatter, DateFormatter,
  ResourceBundle, IDNA, break iterators, and full Transliterator behavior remain
  outside the bounded MVP.
- Grapheme handling is limited to the selected UTF-8 character-slicing cases,
  not full Unicode grapheme cluster segmentation.
- Normalization does not yet implement complete NFC/NFD/NFKC/NFKD ICU parity.
- Locale fallback, ICU version differences, and rich per-object intl error
  state remain future PHPT promotion work.
