# PHPT Extension Policy

Generated from baseline `20260624T210848Z` with 21548 PHPT corpus entries and 20428 known non-green fingerprints.

Extension PHPTs remain in the corpus and full-regression baseline. Policy classification decides whether a non-green result is core-blocking, optional, target-policy, composer-relevant, framework-relevant, or out-of-scope; it does not remove tests from accounting.

## Policy Table

| Extension | Policy | PHPT count | PASS | SKIP | FAIL | BORK | Required for Core | Required for Composer | Needs stub | Needs implementation | Next action |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | --- | --- |
| dom | optional | 879 | 7 | 14 | 851 | 7 | no | no | yes | no | Keep visible in triage; add stubs only when composer/framework tests require them. |
| xml | optional | 65 | 0 | 0 | 64 | 1 | no | no | yes | no | Classify XML parser failures separately from core syntax/runtime failures. |
| simplexml | optional | 157 | 0 | 2 | 155 | 0 | no | no | yes | no | Defer implementation until XML support exists; keep PHPTs counted. |
| pdo | optional | 376 | 0 | 124 | 249 | 3 | no | no | yes | no | Keep database API failures out of core runtime gates while preserving counts. |
| mysqli | optional | 442 | 2 | 4 | 429 | 4 | no | no | yes | no | Treat as database-extension work, not a blocker for core PHPT green. |
| soap | out-of-scope | 589 | 0 | 16 | 567 | 6 | no | no | no | no | Keep failures documented as extension-policy non-green unless scope changes. |
| intl | optional | 477 | 0 | 18 | 458 | 0 | no | no | yes | no | Defer ICU parity; add targeted stubs only for framework smoke blockers. |
| mbstring | composer-relevant | 420 | 3 | 36 | 360 | 21 | no | yes | yes | yes | Plan a bounded UTF-8 string MVP after standard.strings is stable. |
| gd | out-of-scope | 312 | 1 | 55 | 255 | 0 | no | no | no | no | Keep image-processing PHPTs visible but outside core policy-green. |
| phar | composer-relevant | 553 | 3 | 6 | 403 | 141 | no | yes | yes | yes | Define a read-only PHAR MVP after filesystem.streams is stable. |
| opcache | out-of-scope | 593 | 220 | 8 | 364 | 0 | no | no | no | no | Keep Opcache/JIT behavior excluded from runtime correctness scope. |
| session | framework-relevant | 260 | 3 | 0 | 254 | 2 | no | no | yes | yes | Implement deterministic local session state only after filesystem primitives are stable. |
| sapi | target-policy | 347 | 2 | 17 | 254 | 73 | no | no | no | no | Route CLI-compatible tests to phpt.cli and leave CGI/FPM/PHPDBG explicit. |

## Invariants

- Extension PHPT counts come from `tests/phpt/manifests/phpt-corpus.jsonl` and the committed known-failure baseline.
- Extension failures are still present in `docs/phpt/reports/triage.md` and `docs/phpt/reports/full-baseline.md`.
- Out-of-scope means not required for strict core progress; it does not mean silently skipped or deleted.
- Stub or implementation work must be added in the owning functional module, not as generated prompt or phase artifacts.
