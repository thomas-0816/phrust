# Phase 0 Risk Register

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| PHP reference is not pinned exactly. | Later compatibility work can drift against a moving target. | Pin PHP `8.5.7`, tag `php-8.5.7`, and commit in a lockfile. |
| The `php-src` tag exists but the commit is not documented. | Reproducing reference behavior becomes ambiguous. | Resolve and store the commit during reference bootstrap. |
| Nix environment is not reproducible. | Developers and CI may use different tools. | Commit `flake.lock` and require `nix develop`. |
| Scripts work on only one platform. | macOS/Linux support may break early. | Keep scripts POSIX-aware where practical and test inside Nix. |
| Phase creep into lexer or parser work. | Foundation work becomes hard to review and validate. | Keep Phase 0 limited to environment, reference, docs, scripts, and skeletons. |
| License or copying mistakes with `php-src`. | Legal and provenance risk. | Do not commit `php-src`; keep only metadata, paths, hashes, and original docs. |
| Later source or test imports lack provenance. | License obligations can be lost during compatibility work. | Require license/provenance review before importing code or tests from `php-src`. |
| Missing test oracles. | Later implementation can pass local tests while diverging from PHP. | Document token, parse, runtime, and framework smoke oracles in Phase 0. |
| References and Copy-on-Write are underspecified. | Runtime behavior can diverge in subtle aliasing cases. | Treat reference/COW behavior as a top runtime test area. |
| `foreach` mutation behavior is complex. | Array iteration can diverge from PHP in common code. | Build targeted `.phpt`-derived fixtures in later phases. |
| Numeric strings differ from Rust intuition. | Type conversion and comparison behavior can be wrong. | Use reference CLI value tests for conversions and comparisons. |
| Destructor and shutdown order is observable. | Object lifecycle behavior can break frameworks. | Add CLI and `.phpt` shutdown-order tests later. |
| Reflection exactness is hard. | Frameworks can fail even when execution mostly works. | Use Reflection `.phpt` tests and framework smoke tests. |
| Streams, resources, and extensions are broad. | Runtime compatibility scope can expand quickly. | Stage extension coverage and keep Phase 0 boundaries explicit. |

## Review Cadence

This register should be updated whenever a Phase 0 decision changes or a later
phase discovers a new compatibility, reproducibility, or provenance risk.
