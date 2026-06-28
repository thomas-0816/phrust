# Known-Gap Manifests

This directory contains machine-readable mirrors for known-gap policy that was
previously checked only through prose and shell greps.

- `runtime.jsonl` mirrors `docs/runtime-known-gaps.md`.
- `performance.jsonl` mirrors `docs/performance-known-gaps.md`.
- `phpt-runner-smoke.jsonl` owns accepted non-green PHPT runner-smoke outcomes.

Each JSONL row uses the same required fields: `id`, `feature`, `status`,
`layer`, `fixtures`, `reference_behavior`, `current_behavior`, and
`owner_area`. Concrete `fixtures` must exist in the repository. Wildcards and
future examples belong in `fixture_patterns` or `examples`.

Run `just known-gaps` to validate duplicate IDs, expired entries, fixture paths,
documentation references, and accepted PHPT non-green mappings.
