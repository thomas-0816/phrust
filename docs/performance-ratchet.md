# Performance Ratchet

The performance ratchet is the local loop for measuring current speed,
comparing it with an accepted local baseline, classifying regressions and wins,
and generating the next focused performance task.

Raw measurements live under `target/performance/ratchet/` and are not committed.
Committed documentation describes the workflow only.

## Command Surface

```bash
nix develop -c just perf-ratchet-smoke
nix develop -c just perf-ratchet-baseline
nix develop -c just perf-ratchet-current
nix develop -c just perf-ratchet-compare
nix develop -c just perf-ratchet-report
nix develop -c just perf-ratchet-next-prompt
nix develop -c just cli-speed-ratchet
nix develop -c just app-flow-ratchet
nix develop -c just server-responsiveness-ratchet
```

`perf-ratchet-smoke` is the CI-safe shape. It validates prerequisites, runs
cheap CLI/app-flow/server measurements, aggregates available counters, and
validates the ratchet schema. It should fail on broken commands, correctness
failures, invalid artifacts, and missing required smoke output.

`perf-ratchet-baseline` and `perf-ratchet-current` are local decision commands.
They write `baseline.json`/`baseline.md` and `current.json`/`current.md` under
`target/performance/ratchet/`.

`perf-ratchet-compare` compares the two reports and writes
`target/performance/ratchet/compare.json` plus Markdown. Warnings are allowed by
default; strict mode can promote deterministic counter or large wall-clock
regressions:

```bash
PHRUST_RATCHET_STRICT=1 nix develop -c just perf-ratchet-compare
```

## Local Baseline Acceptance

Baseline acceptance is explicit. After correctness holds and the current report
is the baseline you want for future local comparisons:

```bash
PHRUST_RATCHET_ACCEPT=1 nix develop -c just perf-ratchet-accept-local
```

The command refuses to run without `PHRUST_RATCHET_ACCEPT=1` and refuses invalid
or failing current reports.

## Modes

Smoke mode uses one measured iteration, no app-flow scale-up, and concurrency
one for server requests. It is intended to prove the workflow and artifact
schema.

Full mode uses `PHRUST_RATCHET_ITERATIONS`, `PHRUST_RATCHET_WARMUPS`, and
`PHRUST_RATCHET_SCALE` when applicable:

```bash
PHRUST_RATCHET_ITERATIONS=10 \
PHRUST_RATCHET_WARMUPS=3 \
PHRUST_RATCHET_SCALE=2 \
nix develop -c just perf-ratchet-current
```

Server responsiveness accepts:

```bash
PHRUST_SERVER_RESPONSIVENESS_CONCURRENCY=1,4,16
PHRUST_SERVER_RESPONSIVENESS_TIMEOUT=30.0
```

## Interpreting Reports

Wall-clock p50/p95/p99 values are advisory host-local data unless repeated on a
stable machine. Correctness failures are not advisory. A timing result is useful
only after stdout, stderr, exit status, diagnostics, and HTTP response semantics
match expectations.

Use p50 for the typical case, p95 for tail sensitivity, and p99 only when the
sample count is high enough to make it meaningful. Counter and instruction
metrics are less noisy than wall-clock measurements and can be strict in pinned
CI or local decision runs.

## Next Prompt

Run:

```bash
nix develop -c just perf-ratchet-next-prompt
```

The generated `target/performance/ratchet/next-performance-prompt.md` selects one
category: startup, compile/transpile, include/cache, VM execution, server
responsiveness, counter regression, correctness blocker, or measurement gap.
Use it as the next focused task input after reviewing the evidence.

## Avoiding Fake Speedups

Do not hide slowness behind opt-in flags, remove diagnostics, globally disable
fast paths, or compare a changed output with an old timing. Keep a speed change
only when the correctness gates pass and before/after ratchet artifacts show the
target metric improved.
