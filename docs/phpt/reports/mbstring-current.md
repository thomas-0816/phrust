# mbstring Current Focus Report

Text/i18n branch focused mbstring verification.

## Policy

`mbstring` is `required-composer` and `stub-only` for this branch. The selected
surface is Composer/platform-check visibility, not Unicode behavior:

- `extension_loaded("mbstring")` is false.
- `function_exists()` is false for `mb_strlen`, `mb_substr`,
  `mb_strtolower`, `mb_strtoupper`, and `mb_detect_encoding`.
- Direct runtime stub calls fail with `E_PHP_RUNTIME_UNSUPPORTED_MBSTRING`.

This avoids fake success for UTF-8, legacy encoding, case-mapping, and
Oniguruma behavior that is not implemented.

## Classification

| Category | PHPT ownership |
| --- | --- |
| required-core | none in this branch |
| required-composer | generated platform checks and guarded common-function probes |
| required-framework | future guarded probes if a framework fixture proves the need |
| optional | upstream common-function basics once a real encoding strategy exists |
| out-of-scope | exhaustive encoding conversion, regex, mail/MIME, HTTP/input/output translation, exif dependency cases |
| stub-only | selected platform-check surface |
| real-implementation-required | full mbstring parity and any enabled common-function behavior |
| already-implemented | explicit disabled-extension stubs and selected generated PHPTs |

## Selected Manifest

- `tests/phpt/manifests/modules/mbstring.selected.jsonl`
- 3 generated fixtures under `tests/phpt/generated/mbstring/`

## Corpus Snapshot

Committed baseline counts for the broader mbstring-owned corpus:

| Outcome | Count |
| --- | ---: |
| PASS | 3 |
| SKIP | 36 |
| FAIL | 360 |
| BORK | 21 |
| Known non-green | 414 |

## Selected Fixtures

- `tests/phpt/generated/mbstring/platform-checks.phpt`
- `tests/phpt/generated/mbstring/guarded-common-functions.phpt`
- `tests/phpt/generated/mbstring/composer-fallback.phpt`

## First Implementation Slice

The selected slice remains explicit stub-only behavior. A real implementation
slice should start only after choosing an approved Unicode and legacy-encoding
strategy for `mb_strlen`, `mb_substr`, `mb_strtolower`, `mb_strtoupper`, and
`mb_detect_encoding`.

## Target Gates

- `nix develop -c cargo test -p php_runtime`
- `PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mbstring`
- `nix develop -c just verify-stdlib`
- `nix develop -c just verify-phpt`

## Verification

Latest branch verification:

- `nix develop -c cargo test -p php_runtime`: PASS, 186 tests.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src PHPT_DISABLE_REFERENCE_REUSE=1 PHPT_REUSE_LAST=0 PHPT_DEV_REUSE_TARGET_PASS=0 nix develop -c just phpt-dev-module MODULE=mbstring`: PASS, reference 3 PASS and target 3 PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-stdlib`: PASS.
- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src nix develop -c just verify-phpt`: PASS.
