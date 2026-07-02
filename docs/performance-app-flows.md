# Application-Flow Performance Suite

The application-flow suite compares the same deterministic PHP application
flows on Phrust and the pinned/reference PHP CLI. It is correctness-first:
timing rows are reported only after stdout, normalized stderr, and exit status
match the required oracle.

## Fixture Policy

Fixtures live under `tests/fixtures/performance/app_flows/` and are listed in
`manifest.json`. The manifest admits exactly ten scenarios covering routing,
service resolution, templating, configuration bootstrap, validation, hydration,
collections, middleware/events, session/auth policy, and translation lookup.

Admission rules:

- each fixture prints exactly one stable `app-flow ...` line;
- fixture data is deterministic and self-contained;
- fixtures do not use network, databases, filesystem writes, process execution,
  wall-clock time, external package managers, or downloaded application code;
- `PHRUST_APP_FLOW_SCALE` controls larger manual runs while smoke mode uses
  scale `1`;
- a fixture is admitted only when Phrust and reference PHP can both run it
  correctly when the reference binary is available.

Unsupported PHP behavior is avoided in these fixtures instead of becoming a
known-gap row.

## Running

CI-safe smoke mode:

```bash
nix develop -c just app-flow-smoke
```

Full local matrix:

```bash
nix develop -c just app-flow-matrix
```

Manual timeout override for slower debug builds:

```bash
PHRUST_APP_FLOW_TIMEOUT=60.0 nix develop -c just app-flow-matrix
```

Manual reference selection:

```bash
REFERENCE_PHP=third_party/php-src/sapi/cli/php \
  nix develop -c scripts/performance/app_flow_matrix.py \
  --engine target/debug/php-vm --iterations 3 --warmups 1 --scale 2
```

The harness resolves reference PHP from `--reference-php`, then
`REFERENCE_PHP`, then `third_party/php-src/sapi/cli/php`.

## Missing Reference PHP

Smoke mode records `reference_status: skipped` when no reference PHP binary is
available. In that case Phrust fast rows must match the Phrust baseline row, and
the report does not make ratio or speed claims against reference PHP.

Full mode requires reference PHP unless `--allow-missing-reference` is passed
explicitly. Missing reference in full mode without that flag is a hard failure.

## Outputs

The harness writes local artifacts under:

```text
target/performance/app-flows/
```

Committed summary output is:

```text
docs/performance-app-flow-results.md
```

Raw stdout, stderr, status, command lines, counters, and phase timing sidecars
for each scenario and row are local-only under
`target/performance/app-flows/runs/` and must not be committed.

## Reading Results

Each row reports:

- `correctness`: `reference`, `manifest`, or `pass` after oracle comparison;
- `median_ms`, `min_ms`, and `max_ms`: advisory host-local wall-clock samples;
- `ratio_vs_reference`: row median divided by `reference-php-cli` median when
  reference PHP is available;
- `phase_summary`: derived Phrust timing metrics from `--timings-json`,
  including `external_wall_ms`, `internal_total_ms`, `startup_external_ms`,
  `compile_total_ms`, `execute_ms`, `compile_share_percent`, and
  `execute_share_percent`;
- counter highlights: selected non-zero Phrust counters such as quickening,
  inline-cache, output, array, and dispatch-cache counters;
- skip/failure reason for optional or failed rows.

The Markdown matrix includes compile and execute phase columns when timing
sidecars are available. Missing or malformed sidecars are retained in
`timing_warnings` so a local run can distinguish a tooling problem from a real
PHP behavior mismatch.

No strict speed thresholds are enforced. The suite fails on manifest/schema
breakage, fixture divergence, Phrust/reference mismatch when reference is
available, and unexpected command failures.
