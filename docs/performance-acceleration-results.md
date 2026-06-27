# Performance Acceleration Results

Date: 2026-06-27.

This document summarizes the committed Phase 09.16 acceleration matrix policy.
The raw report is generated locally by:

```bash
nix develop -c just acceleration-matrix
```

Generated artifacts:

- `target/performance/acceleration/summary.json`
- `target/performance/acceleration/summary.md`
- `target/performance/acceleration/runs/`

Those files are local evidence only and must not be committed.

## Matrix Dimensions

The default matrix compares each enabled row against `baseline-ir` before it
records advisory timing or counters.

| Row | Default status | Correctness comparison |
| --- | --- | --- |
| `baseline-ir` | Enabled | Source-of-truth row using `--exec-format=ir`, opt level 0, no quickening, no inline caches, no bytecode cache, no JIT, and tiering off. |
| `dense-bytecode-auto` | Enabled | Must preserve stdout, stderr/runtime diagnostics, and exit status while falling back to IR for unsupported dense bytecode. |
| `dense-bytecode-strict` | Enabled for the strict dense-bytecode subset | Runs only fixtures known to lower to strict dense bytecode. Other fixtures are reported as explicit skips. |
| `superinstructions-on` | Enabled for the strict dense-bytecode subset | Compares fused dense opcodes against strict dense bytecode semantics. Other fixtures are explicit skips. |
| `optimizer-level-1` | Enabled | Compares conservative optimizer level 1 against baseline behavior. |
| `optimizer-level-2` | Enabled | Compares conservative optimizer level 2 against baseline behavior. |
| `quickening-on` | Enabled | Compares request-local quickening with fallback counters. |
| `inline-caches-on` | Enabled | Compares guarded inline caches with fallback counters. |
| `all-non-jit` | Enabled | Enables the non-native optimized interpreter stack without Cranelift. |
| `release-all-non-jit` | Optional | Runs when the release `php-vm` binary exists; otherwise records an explicit skip. |
| `jit-cranelift` | Optional | Runs only when requested with `PHRUST_ACCEL_MATRIX_JIT=1` or `--include-jit` and the feature-enabled binary is available. |

The default fixtures are a bounded mix of runtime scalar/function cases,
performance smoke fixtures, output/string fast paths, packed arrays, and
framework-like router/template shapes. Additional fixtures can be supplied with
`scripts/performance/acceleration_matrix.py --fixture <path>`.

## Regression Policy

The matrix is a correctness gate, not a speed gate.

- Any enabled row that changes stdout, stderr/runtime diagnostics, or exit
  status fails the gate.
- Counter sanity is required: counter JSON must be an object and integer
  counters must not be negative.
- Wall-clock timings use warmup/iteration parameters and are reported as
  advisory host-local values only.
- Optional native rows must skip cleanly when not requested or when the local
  binary is unavailable.
- Raw run stdout, stderr, counters, and timing artifacts remain under `target/`.

## Current Optimization Defaults

| Layer | Default | Off switch or selection |
| --- | --- | --- |
| Dense bytecode execution | Off; `ir` remains default | `--exec-format=ir|auto|bytecode` |
| Dense superinstructions | Off | `--superinstructions=off|on` |
| Quickening | Off unless explicitly requested | `--quickening=off|on` |
| Inline caches | Off unless explicitly requested | `--inline-caches=off|on` |
| Optimizer | Explicit opt level | `--opt-level=0|1|2` |
| Bytecode cache | Off unless explicitly requested | `--bytecode-cache=off|read|write|read-write` |
| Release profile | Measurement-only | `just release-benchmark-smoke` and optional matrix row |
| Cranelift | Feature-gated and runtime-off | `jit-cranelift` feature plus explicit `--jit=cranelift` |
| Baseline native tier | Research-only | No runtime switch |

## Compatibility Sweep

Phase 09.17 owns the final compatibility proof. The expected gates are:

```bash
nix develop -c cargo fmt --all --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c just verify-runtime
nix develop -c just verify-stdlib
nix develop -c just verify-performance
nix develop -c just verify-phpt
nix develop -c just phpt-verify-source-integrity
```

If a full PHPT regression is feasible, run:

```bash
PHPT_RUN_FULL=1 nix develop -c just phpt-full-regression
```

Otherwise, run the strongest available focused PHPT modules whose actual names
exist in the current PHPT manifests and record any skipped scope explicitly.

## Latest Local Validation

Validation date: 2026-06-27.

Reference inputs:

- `REFERENCE_PHP=/Volumes/CrucialMusic/src/phrust/third_party/php-src/sapi/cli/php`
- `PHP_SRC_DIR=/Volumes/CrucialMusic/src/phrust/third_party/php-src`

| Gate | Result | Notes |
| --- | --- | --- |
| `nix develop -c cargo fmt --all --check` | PASS | Formatting check completed after the optimizer report test update. |
| `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` | PASS | Workspace clippy completed before the aggregate gates. |
| `REFERENCE_PHP=... nix develop -c just verify-runtime` | PASS | Runtime semantics diff reported 280 total, 230 pass, 50 known gaps, 0 failures. |
| `REFERENCE_PHP=... nix develop -c just verify-stdlib` | PASS | Standard-library docs, coverage, focused diffs, and bridge tests completed. |
| `REFERENCE_PHP=... nix develop -c just verify-performance` | PASS | Includes acceleration matrix: 70 enabled row comparisons and 10 optional/subset skips. |
| `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just verify-phpt` | PASS | Verified 21,548 corpus entries and 20,428 accepted non-green fingerprints. |
| `REFERENCE_PHP=... PHP_SRC_DIR=... nix develop -c just phpt-verify-source-integrity` | PASS | Verified 24,476 php-src manifest entries. |
| `PHPT_RUN_FULL=1 ... nix develop -c just phpt-full-regression` | NOT COMPLETED | Attempted the full 21,548-test corpus. The run reached the SAPI/socket-heavy tail and was stopped after it became impractical for this local pass. |

Focused PHPT fallback:

| Module | Current tree result | Comparison against `origin/performance` |
| --- | --- | --- |
| `operators.conversions` | PASS, 4 reference and 4 target tests green | No regression observed. |
| `standard.arrays` | 184 PASS, 4 SKIP, 12 FAIL in the selected target batch | Same 12 non-green outcomes reproduced on clean `origin/performance`. |
| `strings.literals` | 6 PASS, 3 FAIL in the selected target batch | Same 3 non-green outcomes reproduced on clean `origin/performance`. |
| `arrays.references` | 17 PASS, 58 SKIP, 125 FAIL in an extra target batch | Broader reference-semantics gap batch; sample failures map to committed known-gap families. |

The focused non-green outcomes are not introduced by this performance branch
update, but they are still not a clean PHPT module sweep. The passing aggregate
PHPT baseline verification remains the no-regression gate for the current
committed full baseline.

## Current Known Gaps

- Cranelift remains feature-gated and default-off; optional matrix rows skip
  unless explicitly requested.
- Callgrind smoke is Linux-only and skips on Darwin.
- The safety audit miri smoke skipped locally because `cargo-miri` was present
  but not usable for the active toolchain.
- Focused PHPT module runs still expose pre-existing runtime/stdlib gaps in
  reference-heavy arrays and string-highlighting fixtures; the selected
  `standard.arrays` and `strings.literals` failures reproduce on
  `origin/performance`.
