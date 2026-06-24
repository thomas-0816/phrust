# Phase 7 Cranelift Big-Wins Addendum

Date: 2026-06-23.

Reference target: PHP 8.5.7 (`php-8.5.7`).

This document is the preflight record for the Cranelift big-wins addendum. The
addendum starts from the Phase 7 counter and benchmark surface and keeps the
interpreter as the source of truth. Cranelift remains optional, feature-gated,
and default-off.

## Start Prerequisites

Phase 0 through Phase 6 are present in the repository and remain covered by the
existing phase gates. The Phase 7 implementation is also present beyond the
minimum addendum start point:

| Area | Current status | Evidence |
| --- | --- | --- |
| Phase 0-6 gates and docs | Present | `justfile`, `docs/phase6-final-audit.md`, `docs/known-gaps-phase6.md` |
| Phase 7 prompts 07.00-07.07 | Present | `docs/phase7-preflight.md`, `docs/performance-methodology-phase7.md`, `tests/fixtures/phase7/perf_smoke/`, `scripts/phase7/bench_matrix.py`, `crates/php_vm/src/counters.rs` |
| Phase 7 benchmark smoke | Present | `just bench-phase7-smoke` writes `target/phase7/bench-phase7-smoke.json` |
| Phase 7 counter JSON | Present | `php-vm run --counters-json ...`, `crates/php_vm/src/counters.rs`, `crates/php_perf` |
| Full Phase 7 default gate | Present | `just verify-phase7` |

The repository already contains later general Phase 7 performance work:
bytecode cache, optimizer passes, quickening, inline caches, runtime fast paths,
tiering counters, a default-off JIT experiment, safety audit docs, and CI
policy. The Cranelift addendum must extend that surface instead of replacing it.

## Current JIT And Performance Surface

Existing crates relevant to the addendum:

- `crates/php_jit`: current default-off JIT experiment crate. It already has a
  `jit-cranelift` feature and a conservative eligibility/lowering prototype,
  but it is not yet the backend-neutral API and full Cranelift backend required
  by this addendum.
- `crates/php_perf`: machine-readable performance report helper types.
- `crates/php_bench`: Criterion hot-path benchmarks, excluded from the main
  workspace.
- `crates/php_vm`: VM counters, tiering policy, inline-cache counters,
  quickening counters, and the guarded Phase 7 JIT experiment integration.
- `crates/php_vm_cli`: CLI flags and JSON output used by smoke and diff
  scripts.

Existing commands relevant to the addendum:

```bash
nix develop -c just bench-phase7-smoke
nix develop -c just verify-phase7
nix develop -c just jit-smoke
nix develop -c just jit-cranelift-smoke
nix develop -c just perf-report
nix develop -c cargo test -p php_jit
nix develop -c cargo test -p php_jit --features jit-cranelift
```

`just bench-phase7-smoke` and `just verify-phase7` are already concrete gates,
not placeholders. `bench-phase7-smoke` emits deterministic benchmark JSON with
VM counters for the Phase 7 smoke fixtures.

## Scope And Methodology Records

Prompt 07.CL.01 adds:

- `docs/adr/0780-cranelift-addendum-scope.md` for the addendum scope, risk
  register, default-off policy, and interpreter-fallback stop rule.
- `docs/cranelift-benchmark-methodology.md` for the big-win matrix and
  machine-readable report contract.
- `docs/cranelift-jit-report-schema.md` for the Prompt 07.CL.04 JIT counter
  and report schema.
- `docs/cranelift-clif-dump.md` for the Prompt 07.CL.08 standalone CLIF dump
  and verifier smoke.

## Missing Addendum Building Blocks

These Cranelift-addendum pieces do not exist yet and are intentionally tracked
as follow-up work for prompts after 07.CL.00:

| Required addendum piece | Current state |
| --- | --- |
| `docs/adr/0780-cranelift-addendum-scope.md` | Present |
| `docs/adr/0781-jit-backend-api.md` | Present |
| `docs/adr/0782-cranelift-runtime-abi.md` | Present |
| `docs/adr/0783-cranelift-side-exit-model.md` | Present |
| `docs/adr/0784-cranelift-big-win-eligibility.md` | Missing |
| `docs/adr/0785-cranelift-memory-safety.md` | Present |
| `docs/adr/0786-cranelift-tiering-policy.md` | Present |
| `docs/cranelift-abi-phase7.md` | Present |
| backend-neutral JIT API surface | Present in `php_jit` as `JitBackendApi`, `JitBackendCompileRequest`, `JitBackendCompileOutcome`, `NoopJitBackend`, and `CurrentJitBackend` |
| complete optional Cranelift dependency set | Present behind `jit-cranelift`: `cranelift-codegen`, `cranelift-frontend`, `cranelift-module`, `cranelift-jit`, and `cranelift-native` |
| helper-symbol registry | Present in `php_jit` and documented in `docs/cranelift-helper-symbol-registry.md` |
| Cranelift constrained native backend | Present as `CraneliftNoExecBackend`; it still verifies CLIF for non-executable subsets and additionally compiles the Prompt 07.CL.12 constant-return native subset, the Prompt 07.CL.17 inline checked int add/sub/mul subset, and the Prompt 07.CL.18 simple branch/count-loop subset when native execution is explicitly enabled. Documented in `docs/cranelift-no-exec-backend.md` |
| standalone CLIF dump/verifier smoke | Present as `php-vm dump-cranelift-clif` and `just dump-cranelift-clif`; documented in `docs/cranelift-clif-dump.md` |
| JIT CLI mode flags | Present: `php-vm run --jit=off|noop|cranelift`, `--jit-threshold`, `--jit-max-compile-us`, `--jit-max-functions`, `--jit-eager`, `--jit-dump-clif`, and `--jit-stats=json`; counter JSON reports `jit_mode`, `jit_threshold`, and tiering decisions |
| Int-leaf eligibility analyzer | Present: `php_jit::analyze_jit_eligibility` marks `IntLeafCandidate` functions and emits stable JSON reasons through `--jit=cranelift --jit-stats=json` |
| Cranelift-specific script directory | Present: `scripts/phase7/cranelift/jit_diff.py` |
| Cranelift fixture tree | Present for 07.CL.10 eligibility fixtures under `tests/fixtures/phase7/cranelift/eligibility/`, 07.CL.12 constant-return native fixtures under `tests/fixtures/phase7/cranelift/native/`, 07.CL.13 helper-call fixtures under `tests/fixtures/phase7/cranelift/helper-call/`, 07.CL.14/07.CL.15 side-exit and blacklist fixtures under `tests/fixtures/phase7/cranelift/side-exit/`, 07.CL.16/07.CL.17 int-arithmetic matrix fixtures under `tests/fixtures/phase7/cranelift/int_arithmetic/`, 07.CL.18/07.CL.19 branch/count-loop benchmark fixtures under `tests/fixtures/phase7/cranelift/loops/`, 07.CL.20 through 07.CL.22 packed-array and packed-foreach fixtures under `tests/fixtures/phase7/cranelift/arrays/`, 07.CL.23 known-call fixtures under `tests/fixtures/phase7/cranelift/known-calls/`, 07.CL.24 string-concat fixtures under `tests/fixtures/phase7/cranelift/string-concat/`, 07.CL.25 property-load metadata fixtures under `tests/fixtures/phase7/cranelift/property-load-metadata/`, 07.CL.26 property-load fast-path fixtures under `tests/fixtures/phase7/cranelift/property-load/`, 07.CL.27 method-call metadata fixtures under `tests/fixtures/phase7/cranelift/method-call-metadata/`, 07.CL.28 direct method-call fixtures under `tests/fixtures/phase7/cranelift/method-call/`, and the 07.CL.33 focused lifecycle test `cranelift_native_handle_copy_survives_original_handle_drop` |
| `just verify-phase7-cranelift` | Present; runs `jit-cranelift-smoke`, `jit-cranelift-diff`, `jit-cranelift-bench-smoke`, `jit-cranelift-report`, and `cranelift-guard-report` |
| `just jit-cranelift-smoke` | Present |
| `just jit-cranelift-diff` | Present; compares all fixtures under `tests/fixtures/phase7/cranelift/`, uses test-only `--jit-eager` for Cranelift execution rows, and writes `target/phase7/cranelift/diff.json` |
| `just jit-cranelift-bench-smoke` | Present; validates the int-arithmetic big-win smoke matrix and writes `target/phase7/cranelift/bench-smoke.json` |
| `just jit-cranelift-report` | Present; generates the local Cranelift big-win matrix at `target/phase7/cranelift/big_wins_report.json` |
| `just dump-cranelift-clif` | Present |
| `just cranelift-guard-report` | Present; runs the Big-Win report first, analyzes `target/phase7/cranelift/big_wins_report.json`, and writes `target/phase7/cranelift/guard-report.json` plus `target/phase7/cranelift/guard-report.txt` |
| `just jit-cranelift-disasm` | Present; optional local diagnostic for 07.CL.B that writes FunctionId/IR-fingerprint-linked CLIF and code-size descriptors under `target/phase7/cranelift/disasm/`; documented in `docs/cranelift-disassembly-dumps.md` |
| `just jit-cranelift-fuzz-smoke` | Present; optional 07.CL.C deterministic eligible-IR smoke that generates int-only fixtures, compares `--jit=off` with eager Cranelift, requires zero side exits, and stores seeds in `target/phase7/cranelift/fuzz-smoke.json` |
| `just jit-cranelift-poly-ic-experiment` | Present; optional 07.CL.E local report-only polymorphic IC experiment that compares property/method fixtures with JIT off/on, caps guard entries at four, and feeds megamorphic fallback rows into a guard report extension |
| `just jit-cranelift-framework-smoke` | Present; optional 07.CL.F offline framework-like smoke that generates router, DTO, service, template concat, and config array fixtures under `target/` and reports triggered Big-Win paths |
| Cranelift big-win report JSON | Generated by `just jit-cranelift-report`; `target/phase7/cranelift/big_wins_report.json` must not be committed |

## Addendum Order

The addendum must proceed in this order, validating after each prompt before
starting the next:

1. `07.CL.00`: preflight and known-gap documentation.
2. `07.CL.01`: scope ADR and benchmark methodology.
3. `07.CL.02`: optional Cranelift dependency and feature-gating hardening.
4. `07.CL.03` through `07.CL.09`: backend-neutral API, counters, ABI, helpers,
   no-exec backend, CLIF dump, and CLI flag plumbing.
5. `07.CL.10` through `07.CL.16`: eligibility, diff harness, first constrained
   execution path, helper-call arithmetic, side exits, blacklisting, and first
   benchmark smoke.
6. `07.CL.17` through `07.CL.24`: inline integer paths, simple loops, packed
   arrays, packed foreach, known calls, and string concat.
7. `07.CL.25` through `07.CL.31`: monomorphic property/method metadata and
   fast paths, tiering policy, process-local compile cache, and consolidated
   big-win matrix.
8. `07.CL.32` through `07.CL.36`: guard report, safety audit, CI/Nix hardening,
   result matrix, and final handoff.
9. Optional `07.CL.A` through `07.CL.F`: AOT research, disassembly dumps,
   eligible-IR fuzz smoke, OSR/trace-JIT research
   (`docs/research/cranelift-osr-tracejit-phase7.md`), polymorphic JIT IC
   research (`docs/research/cranelift-polymorphic-ics-phase7.md`), and offline
   framework-like smokes (`docs/cranelift-framework-smokes.md`).

## Prompt 07.CL.12 Native Constant-Return Subset

Prompt 07.CL.12 introduces the first native Cranelift execution path, but keeps
it intentionally narrower than the general int-leaf eligibility analyzer. A
function may compile to native code only when all of these conditions hold:

- native execution is explicitly requested through the JIT API;
- the VM has enabled `--jit=cranelift` and tiering has selected the function;
- the function is an ordinary non-generator leaf with no captures or by-ref
  contract;
- parameters, if any, are plain `int` parameters and the explicit return type
  is `int`;
- the body contains only constant integer loads, moves, no-ops, and an optional
  constant integer return.

The compiled entry is invoked only through `JitFunctionHandle::invoke_i64`,
which checks `JIT_RUNTIME_ABI_HASH` before dispatch. Any compile, ABI, arity,
conversion, or invoke error records the relevant JIT counter and falls back to
the interpreter.

The required native fixtures are:

- `tests/fixtures/phase7/cranelift/native/return-42.php`
- `tests/fixtures/phase7/cranelift/native/function-return-42.php`

Counters now include native compile metadata through `jit_code_bytes` and
`jit_compile_time_nanos` in VM counter JSON. The compact `--jit-stats=json`
payload also exposes `code_bytes` and `compile_time_nanos` for Cranelift runs.

## Prompt 07.CL.13 Helper-Call Int Add/Mul Subset

Prompt 07.CL.13 extends the native subset to simple integer arithmetic through
runtime helpers, not inline machine integer operations. Eligible functions may
load plain `int` parameters or locals, load integer constants, perform `add`
and `mul`, and return an `int` value. Every arithmetic operation is lowered as a
call to the authoritative checked helper:

- `phrust_jit_i64_add_checked(lhs, rhs, out) -> status`
- `phrust_jit_i64_mul_checked(lhs, rhs, out) -> status`

The generated Cranelift code checks the helper status immediately after each
call. A non-zero status returns through the native status/out-pointer ABI, and
the VM records a JIT bailout before resuming the interpreter. This preserves
overflow/error semantics without introducing inline raw integer arithmetic.

The required helper fixtures are:

- `tests/fixtures/phase7/cranelift/helper-call/add-params.php`
- `tests/fixtures/phase7/cranelift/helper-call/add-mul-expression.php`
- `tests/fixtures/phase7/cranelift/helper-call/overflow-add.php`

## Prompt 07.CL.14 Side-Exit ABI And Interpreter Resume

Prompt 07.CL.14 adds `docs/adr/0783-cranelift-side-exit-model.md` and the
stable `SideExitReason` ABI. Runtime invoke failures map to structured
`JitSideExit` metadata before interpreter fallback. VM counter JSON now reports
both `jit_side_exits` and `jit_side_exit_reasons`, while compact
`--jit-stats=json` reports `side_exits` and `side_exit_reasons`.

The Prompt 07.CL.13 helper-call subset uses side exits for non-zero helper
statuses. Prompt 07.CL.17 uses the same structured side-exit machinery for
inline integer overflow. On side exit, the VM discards native output, records
the reason, records a JIT bailout, and re-runs the function through the
interpreter from the normal entry point. Mid-region resume and OSR are not
implemented in these prompts; ambiguous live state remains a JIT ineligibility
condition.

The required side-exit fixture is:

- `tests/fixtures/phase7/cranelift/side-exit/helper-status-overflow.php`

`just cranelift-guard-report` validates that the fixture side-exits with the
expected structured reason, preserves JIT off/on output parity, and writes
`target/phase7/cranelift/guard-report.json`.

## Prompt 07.CL.15 Guard-Failure Blacklisting

Prompt 07.CL.15 adds a process-local JIT blacklist policy for unstable compiled
function keys. The CLI flag is:

```bash
php-vm run --jit=cranelift --jit-blacklist=on script.php
php-vm run --jit=cranelift --jit-blacklist=off script.php
```

The default is `on`. With blacklisting enabled, the VM suppresses further native
attempts for a function key after deterministic thresholds:

- repeated side exits: `too_many_side_exits`;
- repeated guard failures: `guard_failure_rate`;
- compile failure: `compile_errors`;
- ABI mismatch: `abi_mismatch`.

Current Cranelift executable paths do not yet emit inline guard failures, so
the concrete 07.CL.15 fixture uses an int leaf that first executes natively and
then receives string inputs. Those inputs produce `type_mismatch` side exits,
resume through the interpreter, and blacklist the function before a later int
call can execute natively again. `--jit-blacklist=off` keeps retrying for
debugging and preserves PHP-visible output.

The required blacklist fixture is:

- `tests/fixtures/phase7/cranelift/side-exit/unstable-type-switch.php`

VM counter JSON now reports `jit_blacklist_reasons`, and compact
`--jit-stats=json` reports `blacklist`, `blacklisted_regions`, and
`blacklist_reasons`. `just cranelift-guard-report` validates both overflow side
exits and type-switch blacklisting.

## Prompt 07.CL.16 Int-Arithmetic Benchmark Matrix

Prompt 07.CL.16 adds the first benchmarkable Cranelift big-win matrix for
integer leaf functions. The fixture family is:

- `tests/fixtures/phase7/cranelift/int_arithmetic/repeated-function-call-add.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/arithmetic-expression-chain.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/negative-ints.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/boundary-ints.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/counted-loop-accumulator.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/overflow-correctness.php`

`scripts/phase7/cranelift/jit_bench_matrix.py` runs every scenario with
`--jit=off` and `--jit=cranelift`, uses test-only `--jit-eager` for rows that
must exercise native execution immediately, enables `--jit-stats=json`,
supports warmups and repeats, and writes a machine-readable JSON report. The
smoke target runs
the correctness diff before the benchmark and writes:

```bash
nix develop -c just jit-cranelift-bench-smoke
```

The full local report target also runs the correctness diff first:

```bash
nix develop -c just jit-cranelift-report
```

It writes `target/phase7/cranelift/big_wins_report.json`. The initial checked-in
summary table lives in `docs/cranelift-results-phase7.md`; generated JSON under
`target/` remains local output and is not committed.

## Prompt 07.CL.17 Inline Int Fast Path

Prompt 07.CL.17 replaces the executable helper-call path for eligible integer
add/sub/mul with inline checked Cranelift integer operations. The inline path is
only available for functions whose operands are proven `int` by declarations,
prior int-producing operations, or the existing entry guard path. It does not
perform float coercion or weak numeric-string conversion.

The generated native code branches on Cranelift signed overflow results. A
non-overflowing operation stores the integer result and increments
`fast_path_hits`; overflow returns the stable overflow status so the VM records
an `overflow` side exit, increments `overflow_exits` and `slow_path_calls`, and
resumes through the interpreter. Helper symbols remain documented and available
for later non-inline or non-arithmetic fallback paths, but inline integer
add/sub/mul rows now report zero `helper_calls`.

The additional 07.CL.17 fixture coverage is:

- `tests/fixtures/phase7/cranelift/int_arithmetic/negative-ints.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/boundary-ints.php`
- `tests/fixtures/phase7/cranelift/int_arithmetic/overflow-correctness.php`

## Prompt 07.CL.18 Simple Branches And Counted Loops

Prompt 07.CL.18 extends native Cranelift execution from single-block integer
arithmetic to simple multi-block CFGs. The native subset now supports
conditional branches over int comparisons, unconditional jumps, returns, and
counted loops whose loop variable, bound, and body remain in the eligible int
subset. Supported comparison operations are `<`, `<=`, `>`, `>=`, `==`, and
`!=`, plus their strict IR forms where the operands are already proven ints.

Loop lowering remains entry-only: there is no OSR, no mid-loop compilation, and
no attempt to compile loops with calls, array mutation, exception/finally
control flow, or complex `break`/`continue` behavior. Unsupported loop bodies
fall back through stable known-gap rows instead of claiming native execution.

The additional 07.CL.18 fixture coverage is:

- `tests/fixtures/phase7/cranelift/loops/factorial-like-loop.php`
- `tests/fixtures/phase7/cranelift/loops/sum-1-to-n.php`
- `tests/fixtures/phase7/cranelift/loops/branchy-max-min.php`
- `tests/fixtures/phase7/cranelift/loops/non-eligible-loop-call.php`

`jit-cranelift-bench-smoke` requires the simple loop and branch rows to execute
with non-zero `fast_path_hits` and zero `helper_calls`; the call-in-loop row
must fall back with `CL-GAP-LOOP-BODY-CALL`.

## Prompt 07.CL.19 Int Loop Benchmarks

Prompt 07.CL.19 stabilizes the numeric loop benchmark surface without adding a
CI-hard speedup threshold. The Cranelift matrix now includes explicit rows for:

- `repeated_int_function_calls` from
  `tests/fixtures/phase7/cranelift/int_arithmetic/repeated-function-call-add.php`
- `sum_to_n` from `tests/fixtures/phase7/cranelift/loops/sum-1-to-n.php`
- `fib_iterative` from `tests/fixtures/phase7/cranelift/loops/fib-iterative.php`
- `branchy_int_loop` from
  `tests/fixtures/phase7/cranelift/loops/branchy-int-loop.php`

Each row is measured under both `--jit=off` and `--jit=cranelift`, with
correctness diffing still required before the report is accepted. The generated
JSON and `docs/cranelift-results-phase7.md` report compile time, execution time,
total time, side exits, and code bytes separately. `iai-callgrind` is not a
workspace dependency; the existing Phase 7 Valgrind/Callgrind smoke remains a
separate skip-safe gate, so this prompt uses JSON counters and advisory
wall-clock totals.

## Prompt 07.CL.20 Packed Array Layout ABI

Prompt 07.CL.20 prepares read-only packed-array fast paths without adding
native array lowering yet. `docs/cranelift-abi-phase7.md` documents the current
runtime array layout:

- `Value::Array(PhpArray)` owns an opaque COW `Rc<ArrayStorage>`;
- `ArrayStorage` currently contains ordered `Vec<ArrayEntry>` storage,
  `next_append_key`, and `packed_len` metadata;
- the backing vector is not a JIT ABI and must not be read directly from
  generated code.

The read-only helper surface lives in `php_runtime::jit_array`:

- `php_jit_array_is_packed_ints(value) -> status`
- `php_jit_array_len(value, out_len) -> status`
- `php_jit_array_fetch_int_slow(value, index, out_int) -> status`

The helpers guard the current layout version, packed metadata, integer
elements, and reference elements before writing packed length or integer fetch
results. Shared COW storage is accepted for these read-only helper calls.
Non-packed, reference-containing, non-int, and out-of-bounds cases fall back
cleanly. `php_jit` records matching helper symbols in the registry for later
lowering prompts.

The prompt adds packed and mixed array fixtures:

- `tests/fixtures/phase7/cranelift/arrays/packed-array-ints.php`
- `tests/fixtures/phase7/cranelift/arrays/mixed-array-fallback.php`

At this prompt those fixtures validate output parity through fallback. Prompt
07.CL.21 owns the first packed-array fetch fast path and counters.

## Prompt 07.CL.21 Packed Array Int-Index Fetch Fast Path

Prompt 07.CL.21 adds the first executable packed-array path for the shape:

```php
function f(array $xs, int $i): int {
    return $xs[$i];
}
```

Eligibility remains intentionally narrow: the array and index must be required
by-value parameters typed `array` and `int`, the function must return `int`,
the body must be a read-only `FetchDim`, and no reference-returning dim access
or mutation may be present. String-key or untyped-index functions remain
ineligible and fall back through the interpreter.

The Cranelift lowering emits a native status/out-pointer entry that rejects
negative indexes before calling the VM-owned
`php_jit_array_fetch_int_slow` ABI shim. The helper checks packed layout,
integer elements, reference elements, bounds, and writes the integer result only
on success. The VM records:

- `packed_fetch_fast_hits` for successful packed int-index fetches;
- `packed_fetch_bounds_exits` for negative or out-of-bounds integer indexes;
- `packed_fetch_layout_exits` for mixed arrays, non-arrays, non-int elements,
  or reference elements.

Every non-OK native status records a side exit, increments slow-path counters,
and reruns the function through the interpreter, preserving JIT-off output and
diagnostics.

The Prompt 07.CL.21 fixtures are:

- `tests/fixtures/phase7/cranelift/arrays/packed-fetch-valid.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-fetch-out-of-bounds.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-fetch-mixed-array.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-fetch-string-key.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-fetch-negative-index.php`

`jit-cranelift-diff` covers all five edge fixtures. `cranelift-guard-report`
asserts that only the valid packed int-index fixture raises
`packed_fetch_fast_hits`, that bounds cases raise
`packed_fetch_bounds_exits`, and that mixed-layout cases raise
`packed_fetch_layout_exits`. `jit-cranelift-bench-smoke` includes a
`packed_array_int_fetch` row in the big-win matrix.

## Prompt 07.CL.22 Packed Foreach Int-Sum Fast Path

Prompt 07.CL.22 adds the first executable packed-foreach reduction path for
the canonical shape:

```php
function f(array $xs): int {
    $sum = 0;
    foreach ($xs as $x) {
        $sum += $x;
    }
    return $sum;
}
```

The recognizer allows local-name variants but keeps the body and control flow
strict: one typed by-value array parameter, accumulator initialized to integer
zero, read-only by-value foreach iteration, one element local, one integer
addition into the accumulator, and direct accumulator return. By-reference
foreach, mutation of the iterated array, calls, nested control flow, and other
observable operations are non-eligible.

The Cranelift lowering emits an entry-only native loop. It does not OSR into a
running foreach and does not perform mutation or COW writes in native code. The
native path calls the VM-owned packed-array length shim once, then loops over
integer indexes and calls the existing read-only integer fetch shim for each
element. Non-int elements, layout misses, references, and other helper statuses
side-exit before falling back to the interpreter. Checked signed addition
guards overflow and reports the stable `overflow` side-exit reason.

The VM records:

- `packed_foreach_sum_fast_hits` for successful native packed-foreach loops;
- `packed_foreach_sum_layout_exits` for layout, element type, or reference
  guard misses;
- `packed_foreach_sum_overflow_exits` for checked-add overflow exits.

The Prompt 07.CL.22 fixtures are:

- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-all-int.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-mixed-element.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-empty.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-large.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-by-ref-non-eligible.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-body-mutation-non-eligible.php`
- `tests/fixtures/phase7/cranelift/arrays/packed-foreach-sum-overflow.php`

`jit-cranelift-diff` covers the positive, side-exit, empty, large, overflow,
and non-eligible fixtures. `jit-cranelift-bench-smoke` and
`jit-cranelift-report` include `packed_foreach_int_sum`,
`packed_foreach_mixed_element`, and `packed_foreach_overflow` rows with
separate compile, execution, and total timings.

## Prompt 07.CL.23 Known Internal Calls

Prompt 07.CL.23 adds conservative helper-backed native fast paths for exactly
known `strlen($s)` and `count($a)` calls. The recognized function body is a
single straight-line block that loads one by-value parameter, calls the exact
global builtin with one positional argument, and returns the call result.

The recognizer rejects namespace or user-function override ambiguity, named
arguments, unpacked arguments, wrong arity, by-reference or variadic
parameters, and incompatible declared parameter types. Untyped parameters are
accepted only because the native helper performs the PHP-visible string or
array guard and returns a helper-status side exit on mismatch.

The lowering deliberately uses VM-owned guarded helpers instead of reading
runtime object layouts inline. A successful helper call writes the byte length
for `strlen` or the array element count for `count`. Guard misses side-exit and
resume through the ordinary interpreter call, preserving TypeError and argument
count behavior.

The VM records:

- `known_call_fast_hits` for successful guarded known-call native results;
- `known_call_guard_exits` for string/array guard misses;
- `known_call_slow_calls` for fallback calls after known-call guard exits.

The Prompt 07.CL.23 fixtures are:

- `tests/fixtures/phase7/cranelift/known-calls/strlen-valid.php`
- `tests/fixtures/phase7/cranelift/known-calls/strlen-non-string.php`
- `tests/fixtures/phase7/cranelift/known-calls/count-packed.php`
- `tests/fixtures/phase7/cranelift/known-calls/count-mixed.php`
- `tests/fixtures/phase7/cranelift/known-calls/strlen-wrong-arity.php`

`jit-cranelift-diff`, `jit-cranelift-bench-smoke`, and
`jit-cranelift-report` include the valid, guard-exit, packed count, mixed count,
and wrong-arity fixtures.

## Prompt 07.CL.24 Simple String-Concat Fast Path

Prompt 07.CL.24 adds a conservative helper-backed native fast path for exactly
two typed string operands. The recognized function body is a single
straight-line block that loads two by-value parameters declared `string`,
concatenates them with `.` and returns the concat result from a function
declared `string`.

The recognizer rejects untyped operands, non-string parameter declarations,
conversion-sensitive string/int concat, object `__toString`, methods,
closures, generators, captures, by-reference parameters, variadics, defaults,
and any extra observable operation. The VM call-shape gate now allows declared
string returns so the Cranelift eligibility layer, not the legacy int-only
filter, makes the final decision.

The lowering uses the VM-owned
`php_jit_concat_string_string_fast(lhs, rhs, out)` helper. The helper
dereferences PHP references, verifies both effective values are strings,
checks the result length with `checked_add`, reserves the output buffer, copies
the two byte slices, and returns a boxed `Value::String` through the native
status/out-pointer ABI. Guard failure or allocation failure returns a helper
fallback status and the VM resumes through generic concat semantics.

The VM records:

- `string_concat_fast_path_hits` for successful string/string native results;
- `string_concat_fast_path_misses` for helper guard misses before fallback.

The Prompt 07.CL.24 fixtures are:

- `tests/fixtures/phase7/cranelift/string-concat/two-strings.php`
- `tests/fixtures/phase7/cranelift/string-concat/empty-strings.php`
- `tests/fixtures/phase7/cranelift/string-concat/large-strings.php`
- `tests/fixtures/phase7/cranelift/string-concat/template-loop.php`
- `tests/fixtures/phase7/cranelift/string-concat/string-int-slow.php`
- `tests/fixtures/phase7/cranelift/string-concat/object-to-string-slow.php`

`jit-cranelift-diff`, `jit-cranelift-bench-smoke`, and
`jit-cranelift-report` include the two-string, empty, large, template-loop,
string/int fallback, and object `__toString` fallback fixtures.

## Prompt 07.CL.25 Monomorphic Property-Load Metadata

Prompt 07.CL.25 adds metadata collection for future monomorphic property-load
fast paths without enabling inline property loads in Cranelift. VM counter JSON
now emits `property_fetch_profiles`, one deterministic profile per observed
property-fetch callsite. Profiles record:

- receiver class IDs and normalized receiver class names;
- declared property names and source-order property slot indexes;
- visibility contexts and class layout versions;
- public `__get` presence, property-hook presence, and dynamic property
  fallback;
- whether a declared visible property was observed;
- uninitialized typed-property observations;
- monomorphic, polymorphic, or megamorphic callsite state;
- `fast_path_eligible` plus precise non-eligible reasons.

Prompt 07.CL.25 intentionally leaves actual Cranelift property-load lowering to
Prompt 07.CL.26. The interpreter remains the only execution path for property
fetches in this prompt.

The Prompt 07.CL.25 fixtures are:

- `tests/fixtures/phase7/cranelift/property-load-metadata/simple-declared-property.php`
- `tests/fixtures/phase7/cranelift/property-load-metadata/subclass-layout-change.php`
- `tests/fixtures/phase7/cranelift/property-load-metadata/magic-get.php`
- `tests/fixtures/phase7/cranelift/property-load-metadata/property-hook.php`
- `tests/fixtures/phase7/cranelift/property-load-metadata/uninitialized-typed-property.php`

Focused VM tests also cover dynamic property fallback and explicit
polymorphic/megamorphic callsite classification. `jit-cranelift-diff` covers
the fixture set for JIT-off/JIT-on output and diagnostic parity.

## Prompt 07.CL.26 Monomorphic Property-Load Fast Path

Prompt 07.CL.26 adds the first helper-assisted native fast path for simple
monomorphic property reads. The backend only accepts a narrow leaf accessor
shape:

- one required by-value class-typed object parameter;
- a direct load of that parameter followed by a direct property fetch;
- an ordinary by-value return of the fetched property;
- a declared visible instance property with no property hook;
- no public `__get` on the receiver class or its parents.

The generated native entry calls the VM-owned
`php_jit_property_load_monomorphic_fast` helper instead of reading object
storage directly. The VM performs a pre-entry guard over receiver class id and
class-layout version. The helper rechecks the receiver class, fetches the
documented storage name through the runtime object API, rejects missing storage,
and side-exits on uninitialized typed properties before the interpreter raises
the canonical PHP error. Property writes and dynamic properties remain outside
the fast path.

VM counter JSON and compact JIT stats now include:

- `property_load_fast_hits`
- `property_load_guard_exits`
- `property_load_layout_exits`
- `property_load_uninitialized_exits`
- `property_load_slow_calls`

The Prompt 07.CL.26 fixtures are:

- `tests/fixtures/phase7/cranelift/property-load/simple-dto-property-read.php`
- `tests/fixtures/phase7/cranelift/property-load/repeated-property-read-loop.php`
- `tests/fixtures/phase7/cranelift/property-load/wrong-class-side-exit.php`
- `tests/fixtures/phase7/cranelift/property-load/hook-magic-fallback.php`
- `tests/fixtures/phase7/cranelift/property-load/uninitialized-error-path.php`

`cranelift-guard-report` explains the fallback policy for class-guard misses,
hook/magic compile-time fallback, and uninitialized typed-property exits. Local
Prompt 07.CL.26 validation compared 58 diff fixtures and produced benchmark
smoke rows where `property_load_simple_dto` records one property fast hit,
`property_load_dto_loop` records 64 property fast hits, wrong-class dispatch
records one fast hit plus one guard exit, hook/magic records fallback with zero
property fast counters, and uninitialized typed access records one guard exit
plus one uninitialized exit.

## Prompt 07.CL.27 Monomorphic Method-Call Metadata

Prompt 07.CL.27 adds metadata collection for future monomorphic method-call
fast paths without enabling direct method calls from native code. VM counter
JSON now emits `method_call_profiles`, one deterministic profile per observed
instance-method callsite. Profiles record:

- receiver class IDs and normalized receiver class names;
- declaring classes, method IDs, and source-order method slot indexes;
- visibility contexts and override/layout versions;
- final, private, and static method flags;
- public `__call` presence and magic-call fallback observations;
- whether arguments were simple positional arguments;
- by-reference argument or by-reference callee parameter observations;
- whether the resolved callee shape is JIT-eligible or has a direct VM-call
  helper;
- monomorphic, polymorphic, or megamorphic callsite state;
- `fast_path_eligible` plus precise non-eligible reasons.

Prompt 07.CL.27 intentionally leaves direct native method calls to later work.
The interpreter remains the only method execution path, and non-eligible
callsite shapes fall back or reject through the existing VM/lowering
diagnostics.

The Prompt 07.CL.27 fixtures are:

- `tests/fixtures/phase7/cranelift/method-call-metadata/final-method.php`
- `tests/fixtures/phase7/cranelift/method-call-metadata/normal-method-monomorphic.php`
- `tests/fixtures/phase7/cranelift/method-call-metadata/subclass-override.php`
- `tests/fixtures/phase7/cranelift/method-call-metadata/magic-call.php`
- `tests/fixtures/phase7/cranelift/method-call-metadata/by-ref-arg-non-eligible.php`

Focused VM tests cover monomorphic final calls, subclass override
polymorphism, magic `__call` fallback, and counter JSON aggregation for by-ref
non-eligibility. `jit-cranelift-diff` covers the fixture set for JIT-off/JIT-on
output and diagnostic parity.

## Prompt 07.CL.28 Direct Monomorphic Method-Call Fast Path

Prompt 07.CL.28 enables the first direct monomorphic instance-method fast path
for `--jit=cranelift`. The path reuses the Prompt 07.CL.27 method-call
metadata and the request-local method inline-cache table, but it is activated
for Cranelift method calls even when general inline caches are disabled. A
cached target carries:

- receiver class name and receiver class id;
- declaring class and method function id;
- method cache epoch, used as the method/layout version guard;
- visibility scope from the callsite.

Eligible cache hits dispatch through the VM-owned method-call target helper.
That helper invokes the resolved function directly and therefore can enter an
already JIT-compiled callee through the existing function-entry tiering path, or
fall back to the VM call path for non-native callees. The prompt intentionally
does not add inlining or a second native method-call ABI.

The direct path is limited to simple positional instance calls with a stable
receiver class id/name guard, stable method epoch, no by-ref callee parameters,
no named or unpacked arguments, no static-as-instance path, and no public
`__call` fallback path. Guard misses and non-eligible shapes continue through
generic method lookup and increment `direct_call_fallbacks`. Successful direct
helper dispatches increment `direct_call_hits` and the aggregate
`jit_fast_path_hits` counter.

The Prompt 07.CL.28 fixtures are:

- `tests/fixtures/phase7/cranelift/method-call/repeated-small-method.php`
- `tests/fixtures/phase7/cranelift/method-call/subclass-side-exit.php`
- `tests/fixtures/phase7/cranelift/method-call/magic-fallback.php`
- `tests/fixtures/phase7/cranelift/method-call/exception-in-callee.php`
- `tests/fixtures/phase7/cranelift/method-call/service-dto-loop.php`

Focused VM tests cover direct hits after the initial fallback, subclass receiver
fallbacks, magic `__call` fallback, and exception propagation after a direct
hit. `jit-cranelift-bench-smoke` includes the `method_call_service_dto_loop`
row and requires visible `direct_call_hits` plus `direct_call_fallbacks`.

## Prompt 07.CL.29 Tiering Policy From Counters

Prompt 07.CL.29 connects eligibility, counters, guard failures, and native
execution through a conservative Cranelift tiering policy documented in
`docs/adr/0786-cranelift-tiering-policy.md`. The default policy no longer
compiles eligible functions on their first call. Functions remain interpreted
until they satisfy the configured call threshold, optional loop/backedge
threshold, guard stability, blacklist, and compile-budget checks.

The public CLI controls are:

- `--jit-threshold=N` for the minimum function-entry count before compile;
- `--jit-max-compile-us=N` for cumulative compile-time budget;
- `--jit-max-functions=N` for the per-request compiled-function budget;
- `--jit-eager` for tests that need first-call native execution.

Compile-budget or blacklist rejection never changes PHP-visible behavior; it
records the tiering decision and falls back to the interpreter. VM counter JSON
and compact `--jit-stats=json` now expose cold, hot, eager, blacklist, and
budget tiering counters. `scripts/phase7/cranelift/jit_bench_matrix.py`
compares eager correctness rows with threshold rows, including a cold fixture
that must not compile and a hot fixture that must compile after crossing the
threshold.

## Prompt 07.CL.30 Process-Local JIT Compile Cache

Prompt 07.CL.30 adds a VM-owned process-local cache for Cranelift compiled
function handles. The cache is intentionally not persisted to disk. A cache key
records:

- function id;
- deterministic function IR fingerprint;
- `JIT_RUNTIME_ABI_HASH`;
- JIT configuration bits that affect generated code or guards;
- host target ISA string.

Each entry also records the runtime/class layout epoch captured when the handle
was compiled. A lookup with a different epoch invalidates the entry before
falling back to compilation. ABI, IR, config, and target mismatches form
different keys and therefore miss instead of reusing stale native code.
Blacklisting a function invalidates cached handles for that function.

VM counter JSON, compact `--jit-stats=json`, and Cranelift benchmark reports
now surface:

- `jit_compile_cache_hits`;
- `jit_compile_cache_misses`;
- `jit_compile_cache_invalidations`.

Focused VM tests cover reuse of the same function without repeated compilation,
changed IR miss behavior, ABI mismatch miss behavior, and runtime-layout
invalidation. The required Prompt 07.CL.30 gates remain
`jit-cranelift-diff`, `jit-cranelift-bench-smoke`, and the workspace
`jit-cranelift` test run.

## Prompt 07.CL.31 Consolidated Big-Win Matrix

Prompt 07.CL.31 promotes `scripts/phase7/cranelift/jit_bench_matrix.py` from a
flat benchmark list to a consolidated Big-Win matrix report. The generated
`target/phase7/cranelift/big_wins_report.json` uses schema version 2 and records
the required family and path metadata directly in JSON:

- `matrix.required_families` lists the required Big-Win families;
- each row has `matrix_family` and `matrix_family_label`;
- each row has `path_kinds`, including `interpreter_baseline`,
  `jit_helper_call_path`, `jit_fast_path`, `jit_fallback_or_skip`,
  `jit_side_exit_resume`, or `reference_orientation`;
- each row records the normalized command that produced it;
- `reference_php` documents whether optional `REFERENCE_PHP` orientation rows
  were run or skipped.

The script validates the Prompt 07.CL.31 structure before writing a passing
report: int leaf calls, int counted loops, packed int fetch, packed foreach
sum, known `strlen`/`count` calls, string concat, property read loops, and
method call loops must each have interpreter and Cranelift coverage. The
overall matrix must include interpreter baseline, helper-call, and fast-path
coverage. Timings remain informational; there is no hard speedup gate.

The accepted Prompt 07.CL.31 command sequence is:

```bash
nix develop -c just jit-cranelift-diff
nix develop -c just jit-cranelift-bench-smoke
nix develop -c just jit-cranelift-report
```

## Prompt 07.CL.32 Guard Report And Minimization Hook

Prompt 07.CL.32 adds `scripts/phase7/cranelift/guard_failure_report.py`. The
tool consumes the schema-2 Big-Win report JSON instead of re-running or editing
PHP fixtures directly. `just cranelift-guard-report` first regenerates
`target/phase7/cranelift/big_wins_report.json`, then writes:

- `target/phase7/cranelift/guard-report.json`;
- `target/phase7/cranelift/guard-report.txt`.

The report includes top side-exit reasons, high-failure-rate candidates,
blacklisted candidates, and per-row next actions: `keep`, `specialize`,
`blacklist`, or `unsupported`. The action is a triage recommendation, not a
runtime policy change. Existing expected fallback or known-gap rows are
classified as `unsupported`, while guard/helper/type side exits suggest
`specialize`.

The optional minimizer hook is advisory only. If
`scripts/minimize_phase5_failure.py` is present, the report includes commands
that write only under `target/phase7/cranelift/minimized/`; the guard-report
tool itself does not mutate PHP fixtures.

The accepted Prompt 07.CL.32 validation commands are:

```bash
nix develop -c just cranelift-guard-report
nix develop -c just jit-cranelift-report
```

## Prompt 07.CL.33 Through 07.CL.35 Closure Inputs

Prompt 07.CL.33 audits the native Cranelift safety boundary in
`docs/safety-audit-cranelift-phase7.md` and
`docs/adr/0785-cranelift-memory-safety.md`. The audited surfaces are
executable-memory lifecycle, W^X provider status, helper-symbol safety, ABI
layout assumptions, compiled-function lifetime, frame/value pointer validity,
panic behavior, side-exit live-state, drop/destructor interaction, and platform
skip behavior. `jit-cranelift` remains default-off.

Prompt 07.CL.34 hardens local and CI usage. Cranelift commands first run
`scripts/phase7/cranelift/platform_check.py`, which writes
`target/phase7/cranelift/platform.json` with a machine-readable `pass` or
`skip` status. `.github/workflows/phase7.yml` keeps the default Phase 7 job
without Cranelift and adds a separate optional `jit-cranelift` job.

Prompt 07.CL.35 records the result decision matrix in
`docs/cranelift-results-phase7.md`. The current conclusion is deliberately
conservative:

- the all-int packed foreach reduction is the only current keep-and-expand
  performance win;
- integer leaf calls, simple counted loops, packed fetch, known calls,
  typed string concat, and monomorphic property loads remain experimental;
- method-call dispatch and broader object/call/string shapes should be
  revisited after Phase 8/9;
- no implemented row should be removed now because unsupported and side-exit
  rows are useful guardrails;
- every Cranelift native path remains behind the non-default `jit-cranelift`
  feature and explicit runtime mode.

## Prompt 07.CL.36 Final Audit And Phase 7 Handoff

The Cranelift addendum is an optional, default-off extension to the general
Phase 7 performance layer. It does not replace the original Phase 7
baseline/cache/optimizer/quickening/inline-cache work. It adds a concrete
Cranelift evidence path after counters exist.

Implemented Cranelift features:

- optional Cargo/Nix feature gating for Cranelift dependencies;
- backend-neutral JIT API plus no-op and Cranelift backend selection;
- stable JIT counter/report schema and CLI flags;
- `repr(C)` runtime ABI records and helper-symbol registry;
- standalone CLIF dump/verifier smoke;
- conservative eligibility analysis and off-vs-Cranelift differential harness;
- constrained native execution for constant returns, checked integer
  arithmetic, simple branches/count loops, packed-array fetch, packed foreach
  int sum, known `strlen`/`count`, typed string concat, monomorphic property
  loads, and monomorphic method-call dispatch helper paths;
- structured side exits, blacklisting, guard reports, tiering policy, and
  process-local non-persistent compiled-function cache;
- Cranelift safety audit, platform skip contract, optional CI job, and
  machine-readable Big-Win/guard reports.

Disabled or intentionally non-default features:

- Cranelift is not a default Cargo feature and is not selected by default at
  runtime;
- unsupported platforms skip Cranelift addendum gates with
  `target/phase7/cranelift/platform.json`;
- persistent native code cache, ObjectModule/AOT output, OSR, trace JIT,
  polymorphic JIT inline caches, architecture-specific disassembly gates, and
  framework/vendor benchmark suites are not production paths in Phase 7;
- dynamic loop-body calls, conversion-sensitive string concat, magic/hook
  property access, broad method dispatch, and default-on native execution remain
  future work or unsupported fixture rows.

Current report artifacts are generated under `target/phase7/cranelift/` and
must not be committed:

| Artifact | Producer | Purpose |
| --- | --- | --- |
| `platform.json` | `platform_check.py` | Machine-readable host support or skip reason. |
| `diff.json` | `just jit-cranelift-diff` | Off-vs-Cranelift fixture parity evidence. |
| `bench-smoke.json` | `just jit-cranelift-bench-smoke` | Smoke-sized Big-Win matrix rows. |
| `big_wins_report.json` | `just jit-cranelift-report` | Full schema-2 Big-Win matrix and timing/counter rows. |
| `guard-report.json` | `just cranelift-guard-report` | Machine-readable side-exit, blacklist, and recommendation summary. |
| `guard-report.txt` | `just cranelift-guard-report` | Human-readable guard report. |
| `trivial_add.clif` | `just dump-cranelift-clif` | Standalone CLIF verifier smoke output. |
| `disasm/manifest.json` | `just jit-cranelift-disasm` | Optional 07.CL.B local code-size/CLIF dump manifest with FunctionId and IR fingerprint links; native instruction disassembly is explicitly skipped until an object/JIT-memory extraction path exists. |
| `disasm/*.clif`, `disasm/*.json`, `disasm/*.disasm.txt` | `just jit-cranelift-disasm` | Per-scenario optional diagnostic dumps for performance inspection; no default or architecture-specific CI gate depends on them. |
| `fuzz-smoke.json` | `just jit-cranelift-fuzz-smoke` | Optional 07.CL.C deterministic interpreter-vs-Cranelift fuzz smoke report with seeds, grammar coverage, output parity, compile descriptors, and side-exit counters. |
| `fuzz/fixtures/*.php` | `just jit-cranelift-fuzz-smoke` | Generated int-only functions using constants, params, add/sub/mul, comparisons, and branches; fixtures are local artifacts and must not be committed. |
| `polymorphic-ic/report.json` | `just jit-cranelift-poly-ic-experiment` | Optional 07.CL.E local method/property IC research report with output parity, capped guard entries, and default-off policy metadata. |
| `polymorphic-ic/guard-report.json`, `polymorphic-ic/guard-report.txt` | `just jit-cranelift-poly-ic-experiment` | Guard report extension showing polymorphic guard candidates and megamorphic fallback rows. |
| `polymorphic-ic/fixtures/*.php`, `polymorphic-ic/counters/*.json` | `just jit-cranelift-poly-ic-experiment` | Generated local fixtures and VM counter JSON for property/method polymorphic and megamorphic callsites; artifacts must not be committed. |
| `framework-smoke.json` | `just jit-cranelift-framework-smoke` | Optional 07.CL.F offline router/DTO/service/template/config smoke report with output parity and triggered Big-Win path counters. |
| `framework-smoke/fixtures/*.php` | `just jit-cranelift-framework-smoke` | Generated framework-like mini-fixtures; artifacts are local-only and must not be committed. |

Most important benchmark findings:

- the all-int packed foreach reduction is the only current strong local
  Cranelift speed signal;
- most small helper-backed rows are parity/noise or slower once compile/helper
  overhead is visible;
- side-exit and unsupported rows are expected for correctness guardrails and
  should not be counted as speed wins;
- framework-like smokes trigger the current method direct-call, property-load,
  string-concat, and packed-array-fetch paths, but they are offline mini-fixtures
  rather than framework benchmark claims;
- wall-clock timing remains informational only, with no hard performance gates.

The prompt-pack completion map is maintained in
`docs/phase7-cranelift-completion-audit.md`.

Open risks:

- broader workload representativeness is still weak;
- compile overhead is visible for tiny functions;
- object, call, string-conversion, and dynamic loop shapes need richer metadata
  and stronger hot-workload evidence;
- default-on Cranelift would be premature until Phase 8/9 resolves workload,
  W^X/executable-memory policy, crash containment, persistent cache, and
  production lifecycle questions.

Mapping back to the original Phase 7 prompt pack:

| Original Phase 7 area | Addendum effect | Handoff status |
| --- | --- | --- |
| Baseline/Compare | Supplements with Cranelift-specific diff, bench smoke, Big-Win report, and guard report. | Original Prompt 07.08 remains the next prompt for a branch that paused after 07.07 counters. |
| Bytecode-Cache | Not replaced. Cranelift compile cache is process-local and non-persistent only. | Original bytecode-cache prompts remain necessary. |
| Optimizer | Not replaced. Cranelift relies on existing IR/HIR pipeline and does not add optimizer semantics. | Original optimizer prompts remain necessary. |
| Quickening | Supplements hotness/tiering evidence but does not replace interpreter quickening. | Original quickening prompts remain necessary. |
| ICs | Supplements method/property metadata and JIT-side guard decisions. | Original inline-cache prompts remain necessary for interpreter/runtime performance. |
| Safety audit | Supplements the general Phase 7 safety audit with native Cranelift ABI, helper, and executable-memory boundaries. | General cache/JIT/adaptive safety audit remains necessary. |
| CI hardening | Supplements default Phase 7 CI with optional Cranelift feature coverage and skip JSON. | General Phase 7 CI hardening remains necessary. |

Recommended original Phase 7 continuation point: Prompt 07.08,
`Performance-Baseline und Vergleichstool`, for any branch that inserted this
addendum immediately after Prompt 07.07. In this repository the general Phase 7
pack is already implemented, so the practical handoff is to keep using
`nix develop -c just verify-phase7` as the default gate and
`nix develop -c just verify-phase7-cranelift` only for the optional Cranelift
addendum.

## Preflight Validation

Prompt 07.CL.00 validation commands:

```bash
nix develop -c just bench-phase7-smoke
nix develop -c just verify-phase7
```

The first command proves that the Phase 7 benchmark smoke can still produce
counter JSON. The second command proves the existing Phase 7 default gate still
passes after adding these documentation-only preflight artifacts.
