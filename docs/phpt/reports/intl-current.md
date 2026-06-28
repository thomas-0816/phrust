# intl Current Focus Report

Text/i18n branch focused intl verification.

## Policy

`intl` is `optional` and `stub-only` for this branch. The selected surface is
platform-check visibility, not ICU behavior:

- `extension_loaded("intl")` is false.
- `function_exists()` is false for `intl_get_error_code`,
  `grapheme_strlen`, and `normalizer_normalize`.
- `class_exists()` is false for `Locale`, `NumberFormatter`, `Collator`, and
  `IntlChar`.
- Direct runtime stubs for the selected functions fail with
  `E_PHP_RUNTIME_UNSUPPORTED_INTL`.

This keeps Composer/framework probes on fallback paths instead of advertising a
partial ICU surface.

## Classification

| Category | PHPT ownership |
| --- | --- |
| required-core | none in this branch |
| required-composer | negative platform checks when a package requires ext-intl |
| required-framework | guarded fallback probes for common intl classes/functions |
| optional | real ICU-backed intl support |
| out-of-scope | full ICU parity, locale data, converters, calendars, break iterators, transliteration, spoofchecker |
| stub-only | selected platform-check surface |
| real-implementation-required | any enabled intl class/function/constant behavior |
| already-implemented | disabled registry descriptor and generated fallback PHPTs |

## Selected Manifest

- `tests/phpt/manifests/modules/intl.selected.jsonl`
- 3 generated fixtures under `tests/phpt/generated/intl/`

## Corpus Snapshot

Committed baseline counts for the broader intl-owned corpus:

| Outcome | Count |
| --- | ---: |
| PASS | 0 |
| SKIP | 18 |
| FAIL | 458 |
| BORK | 0 |
| Known non-green | 467 |

## Selected Fixtures

- `tests/phpt/generated/intl/platform-checks.phpt`
- `tests/phpt/generated/intl/guarded-common-symbols.phpt`
- `tests/phpt/generated/intl/framework-fallback.phpt`

## First Safe Target

The safe target is disabled platform stubs. A real implementation target should
start with one tightly scoped ICU-backed surface and keep `extension_loaded`
false until the selected behavior is reference-backed.

## Target Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=intl`
- `nix develop -c cargo test -p php_std disabled_text_i18n_extensions_hide_platform_symbols`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`

## Verification

Latest branch verification:

- `nix develop -c cargo test -p php_runtime`: PASS, 186 tests.
- `nix develop -c cargo test -p php_std disabled_text_i18n_extensions_hide_platform_symbols`: PASS, 1 test.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=intl`: PASS, reference 3 PASS and target 3 PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-stdlib`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt`: PASS.
