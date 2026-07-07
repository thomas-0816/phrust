# PHPT Extension Policy

Extension PHPTs remain in the corpus and full-regression baseline. The docs
tree records the policy; generated extension ownership tables are local reports
under `target/phpt-work/reports/extension-policy.md`.

Extension classification is used for triage only. It may explain whether a
failure belongs to core runtime semantics, optional extension work, Composer or
framework compatibility, or an out-of-scope PHP facility, but it must not remove
the PHPT from bookkeeping or hide it from the full-regression baseline.

Source-of-truth files:

- `tests/phpt/manifests/known-gap-catalog.jsonl`
- `tests/phpt/manifests/full-known-failures.jsonl`
- `tests/phpt/manifests/full-baseline-module-counts.jsonl`
- `tests/phpt/manifests/modules/*.json`

Run `nix develop -c just phpt-triage` to regenerate the local reports after
baseline or manifest changes.
