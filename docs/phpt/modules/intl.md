# intl

- Strategy: bounded Unicode helper MVP
- Selected manifest: `tests/phpt/manifests/modules/intl.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility, common symbol
  probes, and bounded helper behavior

## Runtime Contract

- `extension_loaded("intl")` returns `true`.
- `Normalizer::normalize`, `Normalizer::isNormalized`,
  `normalizer_normalize`, and `normalizer_is_normalized` support NFC form only.
- `grapheme_strlen` and `grapheme_substr` operate on UTF-8 scalar values, not
  full Unicode grapheme clusters.
- `transliterator_transliterate` supports a small Latin ASCII slice for
  `Latin-ASCII`, `Any-Latin`, and `Any-Latin; Latin-ASCII`.
- `intl_get_error_code()` returns `0`.

## Required PHPTs

- `tests/phpt/generated/intl/platform-checks.phpt`
- `tests/phpt/generated/intl/guarded-common-symbols.phpt`
- `tests/phpt/generated/intl/framework-fallback.phpt`

## Unsupported Area

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `XML-DOM-INTL-INTL-ICU-DATA` | PHP intl output depends on ICU data, locale databases, and platform ICU version. | Locale, formatter, collator, IDNA, converter, and break-iterator behavior remain unavailable or metadata-only. | `tests/phpt/generated/intl/platform-checks.phpt` | future ICU-backed intl layer |
| `XML-DOM-INTL-INTL-GRAPHEME-SEGMENTATION` | Grapheme APIs use Unicode grapheme cluster rules. | `grapheme_*` uses UTF-8 character boundaries only. | `tests/phpt/generated/intl/framework-fallback.phpt` | future Unicode segmentation layer |
| `XML-DOM-INTL-INTL-NORMALIZATION-FORMS` | Normalizer supports multiple Unicode normalization forms. | Only NFC is accepted; other forms raise `E_PHP_RUNTIME_UNSUPPORTED_INTL`. | `tests/phpt/generated/intl/framework-fallback.phpt` | future Unicode normalization layer |

## Target Gates

- `nix develop -c just phpt-module-target MODULE=intl`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=intl`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`
