# PHPT Extension Policy

Generated from baseline `20260624T210848Z` with 21548 PHPT corpus entries and 20428 known non-green fingerprints.

Extension PHPTs remain in the corpus and full-regression baseline. Policy classification uses `required-core`, `required-composer`, `required-framework`, `optional`, and `out-of-scope`; implementation class uses `stub-only`, `MVP`, `real-implementation-required`, or `already-implemented`. Classification does not remove tests from accounting.

## Policy Table

| Extension | Policy | PHPT count | PASS | SKIP | FAIL | BORK | Top failure clusters | Required for Core | Required for Composer | Framework relevant | Implementation class | Next action |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | --- | --- | --- |
| dom | optional | 879 | 7 | 14 | 851 | 7 | `runtime-error-or-diagnostic` 481; `runtime-unsupported-feature` 390; `needs-triage` 7 | no | no | yes | stub-only | Keep visible in triage; add stubs only when composer/framework tests require them. |
| xml | optional | 65 | 0 | 0 | 64 | 1 | `frontend-parse-or-compile` 30; `runtime-unsupported-feature` 27; `runtime-error-or-diagnostic` 7 | no | no | yes | stub-only | Classify XML parser failures separately from core syntax/runtime failures. |
| simplexml | optional | 157 | 0 | 2 | 155 | 0 | `runtime-unsupported-feature` 80; `runtime-error-or-diagnostic` 77 | no | no | yes | stub-only | Defer implementation until XML support exists; keep PHPTs counted. |
| xsl | optional | 72 | 0 | 0 | 65 | 7 | `runtime-unsupported-feature` 34; `runtime-error-or-diagnostic` 31; `needs-triage` 7 | no | no | no | stub-only | Defer XSLT behavior until DOM/XML support exists; keep PHPTs counted. |
| pdo | optional | 137 | 0 | 117 | 18 | 2 | `runtime-error-or-diagnostic` 97; `runtime-unsupported-feature` 38; `needs-triage` 2 | no | no | yes | stub-only | Keep database API failures out of core runtime gates while preserving counts. |
| pdo_sqlite | required-framework | 80 | 0 | 6 | 73 | 1 | `runtime-error-or-diagnostic` 63; `runtime-unsupported-feature` 16; `needs-triage` 1 | no | no | yes | real-implementation-required | Keep unavailable until PDO core and real SQLite-backed semantics exist. |
| sqlite3 | required-framework | 96 | 0 | 7 | 89 | 0 | `runtime-error-or-diagnostic` 88; `runtime-unsupported-feature` 8 | no | no | yes | real-implementation-required | Choose an approved SQLite dependency and implement real query semantics before enabling. |
| mysqli | required-framework | 442 | 2 | 4 | 429 | 4 | `runtime-unsupported-feature` 258; `runtime-error-or-diagnostic` 179; `needs-triage` 4 | no | no | yes | MVP | Keep the WordPress-oriented, capability-gated mysqli MVP selected in `wp.db-network`; selected prepared-statement flows, report-flagged diagnostic severity, and first-cause DB diagnostics are covered, while broad corpus parity, mysqlnd internals, and thrown `mysqli_sql_exception` parity remain explicit gaps. |
| mysqlnd | out-of-scope | 0 | 0 | 0 | 0 | 0 | none | no | no | no | out-of-scope | No standalone PHPT corpus rows are indexed; keep as out-of-scope driver internals unless MySQL support is requested. |
| soap | out-of-scope | 589 | 0 | 16 | 567 | 6 | `runtime-error-or-diagnostic` 292; `runtime-unsupported-feature` 280; `needs-triage` 4 | no | no | no | out-of-scope | Keep failures documented as extension-policy non-green unless scope changes. |
| intl | optional | 477 | 0 | 18 | 458 | 0 | `runtime-error-or-diagnostic` 376; `runtime-unsupported-feature` 89; `runtime-output-mismatch` 2 | no | no | yes | stub-only | Defer ICU parity; add targeted stubs only for framework smoke blockers. |
| mbstring | required-composer | 420 | 3 | 36 | 360 | 21 | `runtime-error-or-diagnostic` 328; `runtime-unsupported-feature` 46; `needs-triage` 21 | no | yes | yes | MVP | Keep the bounded UTF-8 MVP green; promote broader mbstring behavior only with selected reference-backed fixtures. |
| gd | out-of-scope | 312 | 1 | 55 | 255 | 0 | `runtime-error-or-diagnostic` 273; `runtime-unsupported-feature` 36; `runtime-output-mismatch` 1 | no | no | no | out-of-scope | Keep image-processing PHPTs visible but outside core policy-green. |
| phar | required-composer | 553 | 3 | 6 | 403 | 141 | `runtime-error-or-diagnostic` 349; `needs-triage` 140; `runtime-output-mismatch` 38 | no | yes | yes | real-implementation-required | Define a read-only PHAR MVP after filesystem.streams is stable. |
| opcache | out-of-scope | 593 | 220 | 8 | 364 | 0 | `runtime-error-or-diagnostic` 198; `runtime-output-mismatch` 143; `runtime-unsupported-feature` 107 | no | no | no | out-of-scope | Keep Opcache/JIT behavior excluded from runtime correctness scope. |
| session | required-framework | 260 | 3 | 0 | 254 | 2 | `runtime-error-or-diagnostic` 189; `runtime-unsupported-feature` 65; `runtime-output-mismatch` 5 | no | no | yes | real-implementation-required | Implement deterministic CLI-only session state only after request-local state and superglobals are ready. |
| sapi | out-of-scope | 347 | 2 | 17 | 254 | 73 | `runtime-unsupported-feature` 290; `runtime-error-or-diagnostic` 41; `runtime-output-mismatch` 15 | no | no | no | out-of-scope | Route CLI-compatible tests to phpt.cli and leave CGI/FPM/PHPDBG explicit. |

## Invariants

- Extension PHPT counts come from `tests/phpt/manifests/phpt-corpus.jsonl` and the committed known-failure baseline.
- Extension failures are still present in `docs/phpt/reports/triage.md` and `docs/phpt/reports/full-baseline.md`.
- Out-of-scope means not required for strict core progress; it does not mean silently skipped or deleted.
- Stub or implementation work must be added in the owning functional module, not as generated implementation-history artifacts.
