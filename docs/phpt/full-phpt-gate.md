# PHPT Full PHPT Gate

The Full PHPT gate is mandatory after every module batch. It executes the
complete discovered PHPT corpus and compares the result with the accepted
known-failure baseline.

## Why It Exists

A module batch can be green while the engine regresses unrelated behavior. The
full gate catches new failures, new BORKs, crashes, timeouts, warning
escalations, and changed failure fingerprints outside the active module.

## Outcomes

Module green:

- selected runnable Original PHPT cases for the module pass;
- Derived PHPT and Minimized PHPT cases for the module pass.

Full-run no-regression:

- the complete PHPT corpus runs;
- existing known failures may remain;
- no new unexpected failure or changed fingerprint appears outside the current
  module;
- source integrity still passes.

Final strict green:

- the complete PHPT corpus satisfies the final strict policy;
- no known must-fix runtime failures remain;
- every skip or xfail is documented and justified.

## Artifact Policy

Full-run machine artifacts are written under `target/phpt-work/full-runs/`.
Committed files are limited to manifests, stable generated PHPTs, and concise
workflow/status docs. Local markdown reports are generated under
`target/phpt-work/reports/`.

## PHPT Command

```bash
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression
```

The command runs the complete discovered PHPT corpus from
`tests/phpt/manifests/phpt-corpus.jsonl`, writes machine results to
`target/phpt-work/full-runs/<timestamp>/results.jsonl`, and updates the
committed baseline files plus a local markdown report:

- `tests/phpt/manifests/full-baseline-metadata.json`
- `tests/phpt/manifests/full-baseline-module-counts.jsonl`
- `tests/phpt/manifests/full-known-failures.jsonl`
- `tests/phpt/manifests/known-gap-catalog.jsonl`
- `target/phpt-work/reports/full-baseline.md`

The default target is `target/debug/phrust-php` in `php-cli` mode. This is the
PHP-compatible PHPT target binary. Set
`TARGET_PHP=target/debug/php-vm PHPT_TARGET_MODE=php-vm` only when deliberately
checking the internal developer CLI path.

The full-corpus gate uses a 30 second per-test timeout by default because the
corpus contains stress tests where a 10 second local timeout is load-sensitive.
Override with `PHPT_TIMEOUT_SECONDS=<seconds>` when deliberately tightening or
debugging timeout behavior.

The runner defaults to bounded host parallelism (`min(host CPUs, 8)`). Override
with `PHPT_JOBS=<n>` or `php-phpt-tools run --jobs <n>` to pin a specific
worker count. Results are written in manifest order so the JSONL output remains
deterministic across job counts.

## Fast Iteration

Use the narrow module loop while implementing one runtime area:

```bash
nix develop
just phpt-dev-build
just phpt-dev-module MODULE=standard.strings
```

`phpt-dev-build` builds the PHPT runner and `phrust-php` once with local Cargo
incremental mode enabled and bypasses `sccache`, because `sccache` rejects
incremental compilation. `just phpt-dev-shell` opens one Nix shell, runs that
build once, and leaves the shell open for repeated `just phpt-fast ...` or
`just phpt-rerun-failures ...` commands; when it is already running inside
`nix develop`, it reuses the current environment instead of nesting another
dev shell. Use `phpt-build` for the normal
deterministic cached build path. `phpt-build` and `phpt-dev-build` build the
PHPT runner and target binary in one Cargo invocation so dependency planning and
link setup are paid once.

`phpt-dev-module` runs the selected module against already-built binaries with
`PHPT_SKIP_BUILD=1`, strict previous-result reuse for both Reference PHP and
Target PHP, target-side `PHPT_DEV_REUSE_TARGET_PASS=1`,
`PHPT_TIMEOUT_SECONDS=10`, and `PHPT_WORK_DIR=/private/tmp/phrust-phpt-work` by
default. Strict reuse remains fingerprint based, so a changed runner binary,
target binary, timeout, target mode, PHPT source, external file body, or
expectation text invalidates the cached entry. Target-side dev pass reuse can
reuse unchanged previous PASS results across binary changes; it does not apply
to the Reference PHP comparison.

`phpt-fast` runs the target-only loop with `PHPT_SKIP_BUILD=1`, strict
previous-result reuse, `PHPT_TIMEOUT_SECONDS=3`, and
`PHPT_WORK_DIR=/private/tmp/phrust-phpt-work` by default. Keeping the shell open
avoids paying the `nix develop -c` startup cost for every focused iteration.

The full gate also supports the same build split for local iteration:

```bash
nix develop
just phpt-dev-build
just phpt-full-fast
```

`phpt-full-fast` is the local no-build wrapper around `phpt-full-regression`.
It writes under `/private/tmp/phrust-phpt-work` by default, enables local PASS
reuse, and expects the PHPT runner plus target binary to already exist from
`just phpt-dev-build`. Because it can reuse previous passing results across a
changed target binary, it is an iteration shortcut, not the final committed
baseline gate.

The strict full-corpus gate refuses to start unless `PHPT_RUN_FULL=1` is set.
This prevents accidental 20k+ PHPT runs during local debugging. Use focused
module, file, pattern, or rerun-failures targets while iterating, and reserve
`PHPT_RUN_FULL=1 just phpt-full-regression` for an intentional final
no-regression check.

When previous full-run results exist, `phpt-full-regression` passes the latest
`results.jsonl` to the PHPT tool as a strict reuse cache unless
`PHPT_DISABLE_REUSE=1` is set. The cache key includes target mode, timeout,
target binary fingerprint, PHPT runner fingerprint, PHPT source, external file
body, and expectation text. Reused entries are therefore limited to unchanged
runner/target/test inputs; changed binaries or changed PHPT content are rerun.
Set `PHPT_MANIFEST=<path>` to run the same Reference PHP plus Target PHP module
comparison against a focused manifest without generating or rewriting source
fixtures.

When debugging one failure cluster, narrow the module manifest without writing
generated files into the source tree:

```bash
just phpt-fast MODULE=standard.strings PATTERN=substr_count
just phpt-fast MODULE=standard.strings FILE=ext/standard/tests/strings/substr_count_error.phpt
```

`PATTERN` selects matching manifest lines from the module manifest. `FILE`
creates a one-entry manifest for either an upstream php-src-relative PHPT path
or a repository-local generated PHPT path. Focused runs write their results
below `module-runs/<module>/focus/<selector>/`, so a single-test loop no longer
overwrites the full module `module-runs/<module>/target/results.jsonl` cache.
Set `PHPT_REUSE_LAST=0` when a cold focused run is needed for timing or cache
debugging.

After one full module pass exists, rerun only the latest non-green outcomes:

```bash
just phpt-rerun-failures MODULE=standard.strings
```

The command derives a temporary manifest from the previous module
`results.jsonl`, excluding `PASS`, `SKIP`, and `XFAIL`, then writes the rerun
report below `module-runs/<module>/rerun-failures/`.

For local iteration only, `just phpt-dev-fast ...` and `just phpt-full-fast`
enable `PHPT_DEV_REUSE_PASS=1`. That mode may reuse previous `PASS` results
across a changed target binary when the PHPT input, expectation, target mode,
and timeout are unchanged. It never reuses previous non-green outcomes. Final
verification targets do not set this flag.

`php-phpt-tools run` accepts `--reuse-results <results.jsonl>` or
`PHPT_REUSE_RESULTS=<results.jsonl>`. A result is reused only when the PHPT path,
PHPT source, external `FILE_EXTERNAL` or expectation body, Target PHP binary,
PHPT runner binary, target mode, and timeout match the cached fingerprint. The
output JSONL still contains one result per manifest entry in manifest order.

`just phpt-full-regression` automatically points the runner at the latest
previous full-run `results.jsonl` when one exists. Set `PHPT_DISABLE_REUSE=1`
for a deliberately cold full-corpus run. Runner or VM code changes invalidate
the cache through binary fingerprints, so strict no-regression checks remain
comparable.

For runner-only changes, use the narrower smoke gate first:

```bash
nix develop -c just phpt-runner-smoke
```

That gate covers generic PHPT section handling, expectation variants, expected
failures, runner-provided execution context, and stream-capture handling such as
`CAPTURE_STDIO`. It also checks required-extension capability skips, explicit
SAPI policy skips for `CGI` and `PHPDBG`, plus compressed POST sections that
upstream routes through CGI, when no matching local target capability is
configured, without requiring a full corpus run.

When an accepted known-failure manifest already exists, the command compares the
new full run with that baseline and rejects new or changed failure fingerprints.
Set `PHPT_ACCEPT_BASELINE=1` only when intentionally accepting a new baseline.
The variable must remain explicit; normal verification must not accept new
known failures implicitly.

## Baseline Schema

`full-baseline-metadata.json` is the versioned contract for fresh-checkout
verification. Schema `phpt-full-baseline-v1` contains:

- `timestamp`
- `corpus_count`
- `pass_count`
- `skip_count`
- `fail_count`
- `bork_count`
- `known_failure_count`
- `failure_manifest`

`full-known-failures.jsonl` contains one stable fingerprint per known non-green
case. Each line has these fields:

- `path`
- `module_tag`
- `outcome`
- `failure_fingerprint`
- `primary_missing_feature_guess`
- `owner_module`
- `first_seen_timestamp`

`full-baseline-module-counts.jsonl` contains compact per-module and raw
php-src module PASS/SKIP/FAIL/BORK counts for the same accepted baseline. It
lets `just phpt-triage` reproduce module priorities and extension-policy
counts in a fresh checkout without reading local `target/phpt-work` artifacts.

`known-gap-catalog.jsonl` is schema `phpt-known-gap-v1`. Each row has:

- `id`
- `title`
- `reference_behavior`
- `current_rust_behavior`
- `fixture_or_phpt_example`
- `planned_solution_layer`
- `baseline_count`

Every `primary_missing_feature_guess` in `full-known-failures.jsonl` and every
BORK subclass in `full-baseline-module-counts.jsonl` must have a row here.
`target/phpt-work/reports/known-gaps.md` is the generated human-readable
rendering of the same catalog. `docs/phpt/known-gaps.md` is the stable policy
summary.

`target/phpt-work/reports/full-baseline.md` is the generated local human report
for the same baseline. It must agree with the metadata and machine manifest
when it is present.

## Fresh Checkout Check

```bash
nix develop -c just phpt-verify-baseline
```

The check verifies that:

- report totals match `full-baseline-metadata.json`;
- the corpus manifest count matches `corpus_count`;
- `full-known-failures.jsonl` contains `known_failure_count` entries;
- `FAIL` and `BORK` counts in the manifest match metadata;
- all known failure guess IDs and BORK subclasses are present in
  `known-gap-catalog.jsonl`;
- if the report has any non-green outcomes, the machine-readable known-failure
  manifest is not empty.

`just verify-phpt` includes this check, so a fresh checkout can prove that the
committed Full-PHPT no-regression baseline is internally consistent without
requiring local `target/phpt-work` artifacts.
