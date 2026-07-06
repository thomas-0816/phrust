# Compatibility Governance

Compatibility gaps must be explicit, executable, sortable, and reviewable. The
runtime and PHPT harnesses share one mismatch taxonomy so Batch 1 work can triage
failures without hiding them behind ad hoc text.

## Mismatch Categories

Runtime and PHPT reports use these stable category names:

- `ReferenceParseMismatch`
- `PhrustParseMismatch`
- `CompileMismatch`
- `UnsupportedFeature`
- `RuntimeExitMismatch`
- `StdoutMismatch`
- `StderrMismatch`
- `DiagnosticMismatch`
- `TimeoutOrNontermination`
- `HarnessError`
- `ExpectedKnownGap`
- `UnexpectedPass`

`ExpectedKnownGap` means the failure matched authoritative metadata by explicit
fixture metadata, exact diagnostic ID, or exact fixture path. Keyword matching is
triage-only and must not decide pass/fail status. `UnexpectedPass` means a
known-gap fixture now matches the PHP reference and should be retired or
reclassified with proof.

## Commands

Use the normal Nix shell for validation:

```bash
nix develop -c cargo test -p php_testkit
nix develop -c cargo test -p php_phpt_tools
nix develop -c just known-gaps
nix develop -c just runtime-known-gaps
nix develop -c just runtime-diff
```

`just runtime-diff` writes:

- `target/runtime/runtime-diff/runtime-report.json`
- `target/runtime/runtime-diff/runtime-results.jsonl`
- `target/runtime/runtime-diff/runtime-report.md`
- `target/runtime/reports/runtime-diff-results.jsonl`
- `target/runtime/reports/runtime-diff-report.md`
- one per-fixture JSON file for direct inspection

The grouped reports include category, feature area, diagnostic ID, fixture path,
known-gap ID, first differing line, and suggested owner stream.

## Adding Runtime Fixtures

Add normal executable fixtures below `fixtures/runtime/valid` or
`fixtures/runtime/invalid` when the expected behavior is known. Add governance
seed or deferred examples below `fixtures/runtime/governance` only when the file
exists to exercise the harness/reporting layer rather than to claim a runtime
semantic implementation.

Use metadata in the first lines when needed:

```php
<?php
// runtime-fixture: expect=known_gap known_gap=E_PHP_RUNTIME_WARNING_CHANNEL_COMPAT
```

Supported keys are `expect`, `known_gap`, `args`, `normalize`, and
`php_ref_required`. Governance fixtures may also set `category=<name>` to show
the intended taxonomy bucket without changing pass/fail status. A fixture with
`expect=known_gap` is still executed. When `REFERENCE_PHP` is available and the
outputs match, the report records `UnexpectedPass` instead of failing the run.

## Adding Known Gaps

Runtime gap rows live in `docs/known_gaps/runtime.jsonl` and must stay mirrored
in `docs/runtime/known-gaps.md`.

Every row needs:

- `id`, `feature`, `status`, `layer`, `owner_area`
- `reference_behavior` and `current_behavior`
- at least one concrete `fixtures` path, `fixture_patterns`, `examples`, or
  `fixture_planned=true`

Concrete fixture paths must exist. Wildcards belong in `fixture_patterns`.
Implemented rows must point at positive proof fixtures.

## Retiring Gaps

When a report shows `UnexpectedPass`, confirm the same fixture under
`REFERENCE_PHP`, add or keep the positive proof fixture, update the JSONL row
status and prose table, then run:

```bash
nix develop -c just known-gaps
nix develop -c just runtime-diff
```

Do not delete known-gap metadata until the proof fixture is checked in and the
documentation explains the implemented behavior or the narrower remaining gap.

## Batch 1B and 1C Workflow

Batch 1B should add executable fixtures for the highest-count categories from
`runtime-report.md`, then move matching JSONL rows from planned prose into
fixture-backed known gaps. Batch 1C should retire `UnexpectedPass` entries and
split broad rows when one feature area now has mixed implemented and deferred
behavior.
