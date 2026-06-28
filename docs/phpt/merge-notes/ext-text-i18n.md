# Text and Internationalization Merge Notes

Branch: `phpt/ext-text-i18n`

## mbstring

- Classification: `required-composer`, `stub-only`.
- Selected behavior: `extension_loaded("mbstring")` is false and selected
  common functions remain unavailable to PHP code.
- Runtime direct-call stubs return `E_PHP_RUNTIME_UNSUPPORTED_MBSTRING`.
- First real implementation slice remains blocked on an approved Unicode and
  legacy-encoding strategy.

## intl

- Classification: `optional`, `stub-only`.
- Selected behavior: `extension_loaded("intl")` is false and selected common
  functions/classes remain unavailable to PHP code.
- Runtime direct-call stubs return `E_PHP_RUNTIME_UNSUPPORTED_INTL`.
- Full ICU, locale, collation, normalization, transliteration, formatter,
  converter, and grapheme behavior remain out of scope.
- First real implementation slice should stay disabled until selected
  ICU-backed behavior is reference-backed.

## Merge Risks

- Do not enable either extension in the standard-library registry without a
  corresponding reference-backed implementation slice.
- Do not promote broad upstream `ext/mbstring` or `ext/intl` PHPTs without
  updating the module reports and selected manifests.
- Keep tokenizer PHPTs and unrelated extension manifests out of this branch.

## Closeout Gates

- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mbstring`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=intl`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`

Latest closeout used
`/Volumes/CrucialMusic/src/phrust/third_party/php-src` as the read-only oracle:

- `mbstring` module gate: PASS, reference 3 PASS and target 3 PASS.
- `intl` module gate: PASS, reference 3 PASS and target 3 PASS.
- `verify-stdlib`: PASS.
- `verify-phpt`: PASS.
