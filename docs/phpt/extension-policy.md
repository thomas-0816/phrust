# PHPT Extension Policy

Extension PHPTs remain in the corpus and full-regression baseline. The docs
tree records the policy; generated extension ownership tables are local reports
under `target/phpt-work/reports/extension-policy.md`.

Extension classification is used for triage only. It may explain whether a
failure belongs to core runtime semantics, optional extension work, Composer or
framework compatibility, or an out-of-scope PHP facility, but it must not remove
the PHPT from bookkeeping or hide it from the full-regression baseline.

Source-of-truth files:

- `crates/php_phpt_tools/src/commands/policy.rs` (`EXTENSION_POLICY` — the
  decision rows rendered below)
- `tests/phpt/manifests/known-gap-catalog.jsonl`
- `tests/phpt/manifests/full-known-failures.jsonl`
- `tests/phpt/manifests/full-baseline-module-counts.jsonl`
- `tests/phpt/manifests/modules/*.json`

Run `nix develop -c just phpt-triage` to regenerate the local reports after
baseline or manifest changes.

## Decision rows

One deliberate decision per extension: implement (a bounded MVP is planned),
honest skip (the extension reports as not loaded so skipif sections skip, while
any real capability keeps dispatching), or permanent gap (out of scope; failures
stay counted as extension-policy non-green). Decisions live in
`EXTENSION_POLICY`; this table is the human-readable record of them. Non-green
counts are a snapshot of baseline run `20260712T101615Z` — current numbers come
from the triage reports, never from editing this page.

| Extension | Non-green (20260712) | Decision | State and next action |
| --- | ---: | --- | --- |
| dom | 833 | implement incrementally | Partial implementation with working basic operations; keep visible in triage, add surface as composer/framework tests require it. |
| soap | 546 | permanent gap | Out of scope (network RPC stack); basic class plumbing exists but failures stay documented as extension-policy non-green unless scope changes. |
| phar | 518 | implement (bounded MVP) | Required for Composer; define a read-only PHAR MVP after filesystem.streams is stable. |
| opcache | 432 | permanent gap | phrust has its own bytecode cache and JIT; Zend Opcache/JIT observable behavior stays excluded from runtime correctness scope. |
| intl | 414 | honest skip | Reports as not loaded (skipif parity with a reference build lacking intl) because the class surface (Collator, formatters) was phantom-registered without working methods; real grapheme/normalizer/transliterator functions still dispatch. Full intl needs the class surface. |
| gd | 258 | permanent gap | Image processing stays outside core policy-green; working basics remain visible in triage. |
| mbstring | 248 | implement (bounded MVP) | Required for Composer; bounded UTF-8 MVP exists, extend after standard.strings is stable. |
| pdo | — | implement incrementally | Partial implementation; keep database API failures out of core runtime gates while preserving counts. |
| pdo_sqlite / sqlite3 | 148 | implement (needs decision) | Required for frameworks; blocked on choosing an approved SQLite dependency and real query semantics. |
| mysqli / mysqlnd | 87 | permanent gap | Network database support out of scope unless explicitly accepted. |
| simplexml | 125 | implement incrementally | Partial implementation on top of the XML layer; keep PHPTs counted. |
| xml | — | implement incrementally | Partial parser implementation; classify XML parser failures separately from core syntax/runtime failures. |
| xsl | — | permanent gap (deferred) | Stub-only; defer XSLT until DOM/XML support is complete. |
| session | 127 | implement (bounded MVP) | Required for frameworks; deterministic CLI-only session state after request-local state and superglobals are ready. |
| sapi (cgi/fpm/phpdbg) | — | permanent gap | CLI-compatible tests route to phpt.cli; other SAPIs stay explicit non-goals. |

## Introspection honesty

skipif sections trust `extension_loaded()`, `class_exists()`, and
`function_exists()`. Two failure modes both corrupt the ledger:

- **Claiming absent capability** (the intl case): the extension reported as
  loaded while its classes had no working methods, so skipif ran hundreds of
  tests into fatal errors instead of skipping. Fixed by flipping
  `enabled_by_default` in `fixtures/stdlib/extensions/intl.json` and
  regenerating (`just generate-extension-surfaces`).
- **Hiding present capability**: a class or function that works at runtime but
  is not registered for introspection makes skipif skip tests that would pass —
  registering existing capability is a free unlock. Probes of dom, simplexml,
  xml, pdo_sqlite, sqlite3, mysqli, and gd found their basic operations both
  working and reported, so no such unlock is currently open there.

Every flip of an `enabled_by_default` flag is a policy decision and belongs in
this table with its rationale.
