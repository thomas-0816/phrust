# intl

- Strategy: disabled platform stubs
- Selected manifest: `tests/phpt/manifests/modules/intl.selected.jsonl`
- Selected gate: 3 generated PHPTs covering platform visibility and guarded
  common intl probes
- Corpus snapshot: 477 `intl`-owned candidates in
  `tests/phpt/manifests/phpt-corpus.jsonl`; committed baseline counts are
  0 PASS, 18 SKIP, 458 FAIL, 0 BORK, and 467 known non-green outcomes.

## Decision

Keep `intl` unavailable for now. Do not implement ICU, locale-sensitive
formatting, collation, normalization, transliteration, IDNA, break iterators,
grapheme segmentation, or converter behavior in this branch.

The reference oracle available for this branch reports `intl` as missing, so
the safe platform-check contract is a negative extension check with guarded
symbol probes. Advertising the extension or selected classes/functions without
ICU parity would make Composer/framework checks take code paths that phrust
cannot execute correctly.

## Runtime Contract

- `extension_loaded("intl")` returns `false`.
- `function_exists()` returns `false` for selected common intl probes while the
  extension remains disabled in the standard-library registry.
- `class_exists()` returns `false` for selected common intl classes while the
  extension remains disabled in the standard-library registry.
- Direct internal calls to the registered runtime stubs fail with
  `E_PHP_RUNTIME_UNSUPPORTED_INTL`; they do not return fake ICU state,
  grapheme lengths, or normalization results.
- No locale-sensitive or ICU-backed behavior is implemented by this branch.

## Required PHPTs

Required for this strategy:

- `tests/phpt/generated/intl/platform-checks.phpt`
- `tests/phpt/generated/intl/guarded-common-symbols.phpt`
- `tests/phpt/generated/intl/framework-fallback.phpt`

These PHPTs make the platform decision visible: intl is unavailable, and common
intl helpers must be guarded by `extension_loaded()`, `function_exists()`, or
`class_exists()` checks.

## Optional PHPTs

Optional only after selecting a real ICU-backed implementation strategy:

- Locale and platform basics such as `ext/intl/tests/locale_*`.
- Normalizer and grapheme basics such as `normalizer_normalize` and
  `grapheme_strlen`.
- Formatter basics such as `NumberFormatter` and `IntlDateFormatter`.
- Collator basics.
- `IntlChar` constant and method coverage.

## Out-of-Scope PHPTs

Out of scope for the stub strategy:

- ICU version-sensitive output, locale databases, collation tailoring,
  transliteration rules, IDNA behavior, converter callbacks, and spoofchecker
  behavior.
- Full class surfaces for `IntlBreakIterator`, `ResourceBundle`,
  `MessageFormatter`, `IntlCalendar`, `IntlTimeZone`, `IntlDateFormatter`,
  `NumberFormatter`, `Collator`, `Locale`, `Normalizer`, `IntlChar`, and
  `UConverter`.
- Full error-code and warning text parity.

## ICU Parity Gaps

If this moves from disabled stubs to implementation, the first implementation
must document and test these gaps before enabling `extension_loaded("intl")`:

| Stable ID | Reference behavior summary | Current phrust behavior | Fixture path | Next owner layer |
| --- | --- | --- | --- | --- |
| `PHPT-INTL-UNSUPPORTED-ICU-DATA` | PHP intl output depends on ICU data version and locale database. | `intl` remains disabled and selected classes/functions are unavailable. | `tests/phpt/generated/intl/platform-checks.phpt` | future ICU-backed intl layer |
| `PHPT-INTL-UNSUPPORTED-LOCALE` | Locale fallback and canonicalization must match PHP's intl extension. | `Locale` is unavailable through platform checks. | `tests/phpt/generated/intl/guarded-common-symbols.phpt` | future ICU-backed intl layer |
| `PHPT-INTL-UNSUPPORTED-GRAPHEME-NORMALIZER` | Grapheme and normalization behavior require Unicode segmentation and normalization tables. | `grapheme_strlen` and `normalizer_normalize` are unavailable through platform checks; direct stubs fail with `E_PHP_RUNTIME_UNSUPPORTED_INTL`. | `tests/phpt/generated/intl/guarded-common-symbols.phpt` | future ICU-backed intl layer |
| `PHPT-INTL-UNSUPPORTED-FORMATTER-COLLATOR` | Formatter, collator, date formatter, and converter behavior require ICU-like state and diagnostics. | `NumberFormatter` and `Collator` are unavailable through platform checks. | `tests/phpt/generated/intl/framework-fallback.phpt` | future ICU-backed intl layer |

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=intl`
- `nix develop -c just verify-phpt`
- `nix develop -c just verify-stdlib`
