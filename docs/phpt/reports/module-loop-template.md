# Module Loop Template Report

Template path: `docs/phpt/modules/TEMPLATE.md`.

## Purpose

The template gives future extension branches a consistent module loop without
hand-editing central policy or full-run state. It captures scope, non-scope,
owned files, PHPT manifests, php-src oracle paths, selected and generated PHPTs,
known gaps, local gates, and the full-regression rule.

## Usage

1. Copy the headings into the new module document.
2. Replace placeholders with the module-specific policy and PHPT evidence.
3. Keep original php-src files read-only.
4. Keep generated PHPT provenance complete.
5. Run the focused module gate and `nix develop -c just verify-phpt`.

## Required Invariants

- No original php-src edits.
- No committed `target/` artifacts.
- No new baseline without explicit acceptance.
- Every generated PHPT has provenance.
- Every unsupported behavior has a stable ID, reference behavior, current
  phrust behavior, fixture or PHPT path, and next owner layer.
