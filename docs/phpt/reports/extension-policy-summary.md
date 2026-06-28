# Extension Policy Summary

Generated policy source: `docs/phpt/extension-policy.md`.
Baseline source: `tests/phpt/manifests/full-baseline-metadata.json`.
Machine-readable known-gap source: `tests/phpt/manifests/known-gap-catalog.jsonl`.

## Scope

This report summarizes the extension policy for the remaining PHPT work. It does
not accept a new baseline and does not implement extension behavior.

Tokenizer is excluded from new extension-policy work because it is already
implemented and owned by the existing tokenizer/frontend PHPT slice.

## Classification Rules

Every extension decision must use the central classifications from
`docs/phpt/extension-policy.md`:

- `required-core`
- `required-composer`
- `required-framework`
- `optional`
- `out-of-scope`
- `stub-only`
- `real-implementation-required`
- `already-implemented`

Unsupported extension areas are tracked with stable `extension-policy-*` IDs in
`tests/phpt/manifests/known-gap-catalog.jsonl`. Each ID carries the reference
behavior summary, current phrust behavior, PHPT path or fixture example, and
next owner layer required by the prompt pack.

## Implement Next

The next implementation candidates are policy-prioritized, not baseline
acceptance:

| Extension | Reason | Owner layer |
| --- | --- | --- |
| `mbstring` | Composer and framework checks commonly require real UTF-8 string behavior. | `php_std` / `php_runtime` string builtins |
| `phar` | Composer package workflows require read-only PHAR behavior. | `php_runtime` filesystem/archive layer |
| `session` | Framework smoke tests commonly require deterministic local session state. | `php_runtime` session module |
| `pdo_sqlite` | Framework and offline database smoke tests can use SQLite-backed PDO. | future database extension layer |
| `sqlite3` | Direct SQLite APIs are useful for deterministic local framework-style tests. | future database extension layer |

## Stub-Only Or Optional

The following extensions should remain visible in the full PHPT accounting but
start as stubs or deferred optional work unless a focused prompt requests real
behavior:

| Extension | Policy |
| --- | --- |
| `dom` | `optional`, `stub-only` XML/DOM surface for framework discovery |
| `xml` | `optional`, `stub-only` XML parser surface |
| `simplexml` | `optional`, `stub-only` until XML support exists |
| `xsl` | `optional`, `stub-only` until DOM/XML support exists |
| `pdo` | `optional`, `stub-only` central PDO contracts before drivers |
| `mysqli` | `optional`, `stub-only` database extension surface |
| `intl` | `optional`, `stub-only` ICU-facing surface |

## Explicitly Out Of Scope

These areas are counted and documented but are not required for strict core
progress in the current branch:

| Extension | Reason |
| --- | --- |
| `soap` | SOAP protocol/server behavior is extension-specific and not core runtime correctness. |
| `mysqlnd` | No standalone PHPT corpus rows are indexed; MySQL driver internals are out of scope. |
| `gd` | Image-processing behavior is extension-specific. |
| `opcache` | Opcache/JIT behavior is excluded from runtime correctness scope. |
| `sapi` | CGI/FPM/PHPDBG behavior is target policy, not core CLI behavior. |

## Required Checks

Run these after policy changes:

```bash
nix develop -c just phpt-triage
nix develop -c just phpt-verify-baseline
nix develop -c just verify-phpt
```
