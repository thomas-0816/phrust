# PHPT Reports

This directory contains concise committed summaries for PHPT runs. Raw runner
artifacts, extracted working files, and full result streams belong under
`target/phpt-work/` and must not be committed.

## Full Baseline Contract

`full-baseline.md` is the human-readable summary for the accepted full-corpus
baseline. It is paired with:

- `tests/phpt/manifests/full-baseline-metadata.json`
- `tests/phpt/manifests/full-baseline-module-counts.jsonl`
- `tests/phpt/manifests/full-known-failures.jsonl`
- `tests/phpt/manifests/known-gap-catalog.jsonl`

The metadata file is schema `phpt-full-baseline-v1` and stores timestamp,
corpus count, PASS/SKIP/FAIL/BORK counts, known-failure count, and the failure
manifest path. The JSONL manifest stores stable known non-green fingerprints
with path, module ownership, outcome, missing-feature guess, and first-seen
timestamp.

`full-baseline-module-counts.jsonl` stores compact plan-module, raw php-src
module, and BORK-subclass counts for the same accepted baseline. `just
phpt-triage` uses it when no explicit `PHPT_RESULTS` file is supplied, so a
fresh checkout does not depend on local `target/phpt-work` artifacts to render
module priorities, PASS/SKIP counts, or extension-policy counts.

`known-gap-catalog.jsonl` stores the accepted PHPT known-gap contract. Each row
has an ID, reference behavior, current Rust behavior, fixture or PHPT example,
planned solution layer, and baseline count. `docs/phpt/known-gaps.md` renders
the same data for reviewers.

Run the committed consistency check with:

```bash
nix develop -c just phpt-verify-baseline
```

`just verify-phpt` includes the same check. If `full-baseline.md` reports any
non-green outcomes, the machine-readable known-failure manifest must be present
and non-empty, and every known failure guess plus BORK subclass must be present
in the known-gap catalog.
