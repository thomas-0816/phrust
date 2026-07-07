# PHPT Module Workflow

This directory contains stable PHPT module workflow notes and templates. The
current generated module index and per-module status pages are local reports
under `target/phpt-work/modules/`.

Use `TEMPLATE.md` when starting a focused PHPT module loop, then keep permanent
decisions in manifests, known-gap rows, or source-specific behavior notes rather
than committing regenerated status tables.

Run `nix develop -c just phpt-triage` to refresh generated module reports after
baseline, manifest, or corpus changes.
