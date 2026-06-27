# Performance Cranelift Addendum Completion Audit

Date: 2026-06-24.

This audit covers every required work item `07.CL.00` through `07.CL.36` and every
optional work item `07.CL.A` through `07.CL.F`. Cranelift remains a non-default
Cargo feature and a non-default runtime mode.

## Requirement Map

| work item | Completion evidence |
| --- | --- |
| `07.CL.00` Preflight | `docs/performance-cranelift-big-wins.md` records start state, existing Performance commands/crates, missing Cranelift pieces, and addendum order. `docs/performance-cranelift-known-gaps.md` tracks Cranelift-specific gaps. |
| `07.CL.01` Scope ADR/methodology | `docs/adr/0780-cranelift-addendum-scope.md` keeps the interpreter authoritative, Cranelift default-off, eligible-only, and fallback-required. `docs/cranelift-benchmark-methodology.md` documents JIT off/on A/B reports, warmup, counter JSON, and no hard wall-clock CI gate. |
| `07.CL.02` Feature gating | `crates/php_jit/Cargo.toml` gates Cranelift crates behind `jit-cranelift`; default builds omit it. `just jit-cranelift-smoke` and `scripts/performance/cranelift/platform_check.py` validate feature-on or skip-safe behavior. |
| `07.CL.03` Backend API | `crates/php_jit/src/backend.rs` defines the backend-neutral API, no-op backend, selected backend adapter, compile request/outcome, status, and error surfaces. |
| `07.CL.04` Counters/schema | `crates/php_vm/src/counters.rs`, `crates/php_perf`, and `docs/cranelift-jit-report-schema.md` expose stable JIT counters, compact stats JSON, report rows, and compile descriptors. |
| `07.CL.05` Runtime ABI | `crates/php_jit/src/abi.rs`, `docs/adr/0782-cranelift-runtime-abi.md`, and `docs/performance-cranelift-abi.md` define `repr(C)` value/frame/exit records, ABI version/hash, and layout tests. |
| `07.CL.06` Helper registry | `crates/php_jit/src/helpers.rs` and `docs/cranelift-helper-symbol-registry.md` define stable helper ids, symbol names, signatures, ABI hash, and lookup tests. |
| `07.CL.07` No-exec backend | `crates/php_jit/src/cranelift_lowering.rs` and `docs/cranelift-no-exec-backend.md` provide the Cranelift backend skeleton, non-exec verification, and typed errors. |
| `07.CL.08` CLIF dump/verifier | `php-vm dump-cranelift-clif`, `just dump-cranelift-clif`, and `docs/cranelift-clif-dump.md` generate and verify `target/performance/cranelift/trivial_add.clif`. |
| `07.CL.09` CLI flags | `crates/php_vm_cli/src/main.rs` and `crates/php_vm/src/vm.rs` provide `--jit=off|noop|cranelift`, tiering flags, `--jit-eager`, `--jit-stats=json`, `--jit-dump-clif`, and safe fallback behavior. |
| `07.CL.10` Eligibility | `php_jit::analyze_jit_eligibility`, eligibility fixtures, and `--jit-stats=json` report typed int-leaf candidates and stable rejection reasons. |
| `07.CL.11` Diff harness | `scripts/performance/cranelift/jit_diff.py` and `just jit-cranelift-diff` compare JIT off/on across Cranelift fixtures and write `target/performance/cranelift/diff.json`. |
| `07.CL.12` Constant native execution | Cranelift native handles execute the narrow constant-int return subset with ABI checks, counters, fallback, and fixtures under `tests/fixtures/performance/cranelift/native/`. |
| `07.CL.13` Helper add/mul | Helper-backed int add/mul lowering, overflow status fallback, helper counters, and fixtures under `tests/fixtures/performance/cranelift/helper-call/` are covered by diff and smoke gates. |
| `07.CL.14` Side exits | `docs/adr/0783-cranelift-side-exit-model.md`, side-exit counters/reasons, and side-exit fixtures validate structured resume to the interpreter. |
| `07.CL.15` Blacklisting | JIT blacklist options/counters and `tests/fixtures/performance/cranelift/side-exit/unstable-type-switch.php` validate repeated-failure suppression without output changes. |
| `07.CL.16` First bench smoke | `scripts/performance/cranelift/jit_bench_matrix.py`, `just jit-cranelift-bench-smoke`, and `target/performance/cranelift/bench-smoke.json` cover the first int/helper matrix. |
| `07.CL.17` Inline int fast path | Inline checked int add/sub/mul paths, overflow exits, fast-path counters, and int-arithmetic fixtures validate native execution and fallback. |
| `07.CL.18` Branches/count loops | Simple branch and counted-loop lowering is covered by loop fixtures, `jit-cranelift-diff`, bench smoke, and the Big-Win report. |
| `07.CL.19` Loop benchmarks | Loop benchmark rows and optional instruction-count smoke policy are documented in `docs/performance-cranelift-results.md` and `docs/cranelift-benchmark-methodology.md`; speed remains non-gating. |
| `07.CL.20` Packed array ABI | `php_runtime::jit_array`, helper registry entries, `docs/performance-cranelift-abi.md`, and array fixtures define and test the conservative read-only packed-array ABI. |
| `07.CL.21` Packed int-index fetch | Packed array int-index fetch uses helper-owned guards and counters with valid, bounds, mixed, string-key, and negative-index fixtures. |
| `07.CL.22` Packed foreach int sum | Packed foreach int-sum native loop covers all-int, mixed, empty, large, by-ref/mutation non-eligible, and overflow fixtures with fast-hit/layout/overflow counters. |
| `07.CL.23` Known calls | Exact one-argument `strlen` and `count` helper paths are covered by valid, guard-exit, mixed/packed, and wrong-arity fixtures. |
| `07.CL.24` String concat | Typed string/string concat helper path is covered by two-string, empty, large, template-loop, string/int fallback, and object `__toString` fallback fixtures. |
| `07.CL.25` Property metadata | VM counter JSON reports property profiles with class ids, slots, layout versions, hook/magic/dynamic/uninitialized metadata, state, eligibility, and reasons. |
| `07.CL.26` Property fast path | Helper-assisted monomorphic DTO property reads are covered by simple, loop, wrong-class, hook/magic, and uninitialized fixtures plus guard counters. |
| `07.CL.27` Method metadata | VM counter JSON reports method profiles with class/method ids, slots, visibility, override versions, magic/by-ref reasons, state, and eligibility. |
| `07.CL.28` Direct method fast path | Guarded monomorphic method dispatch helper covers repeated calls, subclass fallback, magic fallback, exception propagation, and service/DTO loop fixtures. |
| `07.CL.29` Tiering | `docs/adr/0786-cranelift-tiering-policy.md`, tiering options/counters, hot/cold/eager rows, budget and blacklist rejection accounting validate conservative compile policy. |
| `07.CL.30` Compile cache | Process-local non-persistent compiled-function cache is keyed by function id, IR fingerprint, ABI/config hash, target ISA, and runtime epoch; focused VM tests cover reuse and invalidation. |
| `07.CL.31` Consolidated matrix | `just jit-cranelift-report` writes schema-2 `target/performance/cranelift/big_wins_report.json` with all required families and path kinds. |
| `07.CL.32` Guard report | `scripts/performance/cranelift/guard_failure_report.py` and `just cranelift-guard-report` write JSON/text summaries with side exits, blacklists, recommendations, and minimizer hooks. |
| `07.CL.33` Safety audit | `docs/safety-audit-cranelift.md`, `docs/adr/0785-cranelift-memory-safety.md`, unsafe boundary comments, and native-handle lifetime tests audit JIT memory, ABI, helpers, side exits, and destructors. |
| `07.CL.34` Nix/CI hardening | `scripts/performance/cranelift/platform_check.py`, `.github/workflows/ci.yml`, `docs/performance-ci-policy.md`, and skip JSON isolate optional Cranelift gates from default Performance validation. |
| `07.CL.35` Results matrix | `docs/performance-cranelift-results.md` links reports and classifies each feature by correctness, speed indication, compile overhead, side exits, safety, default status, and recommendation. |
| `07.CL.36` Final audit/handoff | `docs/performance-cranelift-big-wins.md`, `docs/performance-cranelift-known-gaps.md`, and this audit record implemented features, disabled features, reports, risks, and how Cranelift supplements original Performance work. |
| Optional `07.CL.A` ObjectModule/AOT | `docs/research/cranelift-objectmodule-aot.md` compares ObjectModule/JITModule, cache keys, ABI hash, security and invalidation risks, and recommends no persistent native cache in Performance. |
| Optional `07.CL.B` Disassembly/code-size dumps | `docs/cranelift-disassembly-dumps.md`, `scripts/performance/cranelift/disasm_dump.py`, and `just jit-cranelift-disasm` write optional FunctionId/IR-fingerprint/CLIF/code-size artifacts under `target/performance/cranelift/disasm/` without runtime or CI coupling. |
| Optional `07.CL.C` Eligible-IR fuzz smoke | `scripts/performance/cranelift/jit_eligible_ir_fuzz_smoke.py` and `just jit-cranelift-fuzz-smoke` generate deterministic int-only fixtures, compare JIT off/on, require zero side exits, and save seeds in `fuzz-smoke.json`. |
| Optional `07.CL.D` OSR/trace-JIT research | `docs/research/cranelift-osr-tracejit.md` documents live-state maps, loop headers, deopt snapshots, exception/destructor handling, foreach state, trace-JIT tradeoffs, and recommends no Performance implementation. |
| Optional `07.CL.E` Polymorphic IC research | `docs/research/cranelift-polymorphic-ics.md`, `scripts/performance/cranelift/polymorphic_ic_experiment.py`, and `just jit-cranelift-poly-ic-experiment` cap experimental entries at four, compare local property/method fixtures, and extend the guard report with megamorphic fallback evidence while staying default-off. |
| Optional `07.CL.F` Framework-like smokes | `docs/cranelift-framework-smokes.md`, `scripts/performance/cranelift/framework_smoke.py`, and `just jit-cranelift-framework-smoke` generate offline router, DTO, service, template concat, and config array fixtures, compare JIT off/on, and report triggered Big-Win paths. |

## Generated Evidence

All generated reports are local artifacts under `target/performance/cranelift/` and
must not be committed:

- `diff.json`
- `bench-smoke.json`
- `big_wins_report.json`
- `guard-report.json`
- `guard-report.txt`
- `disasm/manifest.json`
- `fuzz-smoke.json`
- `polymorphic-ic/report.json`
- `polymorphic-ic/guard-report.json`
- `framework-smoke.json`

## Validation Results

Current validation was refreshed in `nix develop` on 2026-06-27 for the
Performance acceleration Phase 09.14 audit:

| Command | Result |
| --- | --- |
| `nix develop -c python3 -m py_compile scripts/performance/cranelift/disasm_dump.py scripts/performance/cranelift/framework_smoke.py scripts/performance/cranelift/guard_failure_report.py scripts/performance/cranelift/jit_eligible_ir_fuzz_smoke.py scripts/performance/cranelift/polymorphic_ic_experiment.py` | Passed. |
| `nix develop -c cargo fmt --check` | Passed. |
| `nix develop -c git diff --check` | Passed. |
| `nix develop -c just verify-performance` | Passed. Optional Callgrind and Miri surfaces skipped with explicit Darwin/toolchain skip messages. |
| `nix develop -c just verify-cranelift` | Passed. Included feature smoke, 68-fixture diff, 70-row bench smoke, full report, guard report, and eligible-IR fuzz smoke. |
| `nix develop -c just jit-cranelift-diff` | Passed; wrote `target/performance/cranelift/diff.json` after comparing 68 fixtures. |
| `nix develop -c just jit-cranelift-bench-smoke` | Passed; wrote `target/performance/cranelift/bench-smoke.json` with 70 rows. |
| `nix develop -c just jit-cranelift-report` | Passed; wrote `target/performance/cranelift/big_wins_report.json` with 70 rows and all eight required families. |
| `nix develop -c just cranelift-guard-report` | Passed; wrote `target/performance/cranelift/guard-report.json` and `.txt` with keep/specialize/unsupported/blacklist recommendations. |
| `nix develop -c just jit-cranelift-disasm` | Passed; wrote five linked entries under `target/performance/cranelift/disasm/`; native instruction disassembly remains explicitly skipped. |
| `nix develop -c just jit-cranelift-poly-ic-experiment` | Passed; wrote `polymorphic-ic/report.json` and guard report extension with property and method `megamorphic_fallback` rows. |
| `nix develop -c just jit-cranelift-framework-smoke` | Passed; wrote `framework-smoke.json` for router dispatch, DTO hydration, service method loop, template-like string concat, and config array reads. |

Generated report readback:

- `big_wins_report.json`: `status=pass`, 70 rows, all required matrix families.
- `disasm/manifest.json`: `status=pass`, five entries, native disassembly
  explicitly skipped.
- `fuzz-smoke.json`: `status=pass`, seeds `118226945` and `118226946`, 12
  cases, no failures.
- `polymorphic-ic/report.json`: `status=pass`, max entries `4`, no failures,
  property and method megamorphic fallback rows.
- `framework-smoke.json`: `status=pass`, five fixture kinds, triggered
  `method_direct_call`, `packed_array_fetch`, `property_load`, and
  `string_concat`.
