# intl Current Focus Report

Focused bounded intl harness:

| Outcome | Count |
| --- | ---: |
| PASS | 3 |
| FAIL | 0 |
| SKIP | 0 |
| BORK | 0 |

## Selected Fixtures

- `tests/phpt/generated/intl/platform-checks.phpt`
- `tests/phpt/generated/intl/guarded-common-symbols.phpt`
- `tests/phpt/generated/intl/framework-fallback.phpt`

## Current Policy

The `intl` extension is enabled for bounded NFC normalizer helpers,
UTF-8-character `grapheme_*` helpers, a small Latin ASCII transliterator slice,
and `intl_get_error_code()`. ICU data, locale behavior, formatter/collator
parity, IDNA, break iterators, and full grapheme cluster segmentation remain
documented gaps.
