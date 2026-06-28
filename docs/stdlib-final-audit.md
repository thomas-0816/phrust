# Standard library Final Audit

Reference target: PHP 8.5.7 (`php-8.5.7`).

This audit is the Work item handoff artifact. It records the required
Standard library gates, the optional coverage that was implemented in-tree, and the
remaining known gaps carried as Performance input.

## Required Gates

Run these before closing Standard library:

```bash
nix develop -c just verify-stdlib
nix develop -c just performance-tests
nix develop -c just diff-stdlib
nix develop -c just diff-streams
nix develop -c just diff-json-pcre-date
nix develop -c just diff-spl-reflection
nix develop -c just composer-smoke
nix develop -c just stdlib-coverage
```

The stream, JSON/PCRE/Date, and SPL/Reflection gates are real
`scripts/stdlib_diff.py` differential runs over dedicated fixture areas. They
must not be replaced by placeholder skip scripts.

## Fixture Coverage

- `tests/fixtures/stdlib/_harness/stdlib`: broad standard-library MVP
  differential fixtures, including optional `hash`, `hash_hmac`,
  `random_bytes`, and `random_int` shape/range coverage.
- `tests/fixtures/stdlib/_harness/streams`: resource, `php://memory`, and
  local filesystem path smoke fixtures.
- `tests/fixtures/stdlib/_harness/json-pcre-date`: JSON, PCRE, and Date/Time
  extension smoke fixtures.
- `tests/fixtures/stdlib/_harness/spl-reflection`: SPL iterator/container and
  Reflection smoke fixtures.
- `tests/fixtures/stdlib/corpus`: Composer/framework-style regression snippets
  for autoload, environment, JSON config, routing, DateTime/version parsing,
  arrays, and reflection attributes.

## Optional Coverage

- Work item: reference metadata extraction exists through
  `scripts/stdlib/list_reference_functions.php`,
  `scripts/stdlib/list_reference_classes.php`,
  `scripts/stdlib/list_reference_constants.php`, and
  `scripts/stdlib/function_coverage.py`. Committed arginfo generation is
  available through `just generate-arginfo`, and strict snapshot drift
  verification is available through `just verify-generated-arginfo`;
  `performance-tests` runs the generator against a local php-src-style fixture
  with manual overrides.
- Work item: PHAR remains governed by ADR-0066. Composer source mode is the
  required path; read-only PHAR support is not enabled in Standard library.
- Work item: tokenizer extension metadata and runtime smoke coverage are
  included in `performance-tests`.
- Work item: online Composer and Packagist access are default-off; local
  source-mode Composer smoke is available through `composer-smoke-source`.
- Work item: hash/random MVP is implemented and covered by
  `STDLIB_HASH_RANDOM`.
- Work item: larger Composer source checkouts are opt-in through
  `PHRUST_STDLIB_COMPOSER_SOURCE_DIR` and skip explicitly when absent.

## Performance Handoff

Performance should consume `docs/stdlib-known-gaps.md`,
`docs/function-stdlib-coverage.md`, and `docs/extension-stdlib-coverage.md` as
the authoritative gap map. Highest-priority carryovers are full arginfo
generation/import, byte-perfect extension diagnostics, full Date/Time timelib
parity, complete hash algorithm coverage, PHAR only if ADR-0066 is superseded,
and broader upstream PHPT promotion.
