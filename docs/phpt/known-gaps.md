# PHPT Known Gaps

This page defines how PHPT known gaps are tracked. Generated gap tables are not
committed documentation; `nix develop -c just phpt-triage` writes the current
human-readable report to `target/phpt-work/reports/known-gaps.md`.

The machine-readable source of truth is
`tests/phpt/manifests/known-gap-catalog.jsonl`. Each accepted gap needs a stable
identifier, a reference behavior summary, the current phrust behavior, a fixture
or PHPT example, the planned solution layer, and the baseline count when it is
derived from the full PHPT baseline.

Runner-smoke known-gap rows are mirrored in
`docs/known_gaps/phpt-runner-smoke.jsonl` so the generic known-gap validator can
check the PHPT smoke contract without reading generated reports.

The full-regression manifest remains authoritative for unexpected failures.
Known gaps may explain accepted failures, but they do not remove those failures
from the corpus or from module accounting.
