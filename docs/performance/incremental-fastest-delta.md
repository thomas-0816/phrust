# Incremental Fastest-Engine Delta Guard

Date: 2026-07-06.

This is a current-`main` delta guard for the incremental fastest-engine tranche.
Its job is to stop plausible-looking work from *reimplementing features that
already exist*. Before touching any fastest-engine surface, confirm here that the
work is still a real delta.

It is a rendered projection of the authoritative catalog in
`docs/performance/fastest-engine-known-gaps.md` (the `FPE-GAP-*` rows) plus the
research docs under `docs/research/`. Where the two disagree, the `FPE-GAP-*`
catalog and the code win; regenerate this doc, do not hand-drift it.

Generated evidence under `target/performance/` is local only and is never
committed.

## Status legend

- **present** — implemented and executing on the managed-fast/default path.
- **partial** — implemented for a bounded subset; a named larger form is still
  open behind guards/fallbacks.
- **report-only** — metadata/counters/reports exist; no execution change. The
  feature deliberately does not alter PHP-visible behavior yet.
- **optional/default-off** — implemented behind an explicit flag or feature; the
  default path is unchanged and separately measured.
- **absent** — not productized in the repo.

## Classification

| Fastest-engine idea | Status | Concrete evidence (code / docs / tests / counters) | Smallest remaining delta | Owning pack prompt |
| --- | --- | --- | --- | --- |
| Integrated server, no external PHP | present | `crates/php_server`, `phrust-php -S`; executes through phrust frontend/runtime/VM in-process. `docs/server-architecture.md`, `docs/web-server.md`. | Improve engine hot path / cache scalability metrics, **not** process orchestration. | P10 |
| CLI bytecode cache | present | `crates/php_bytecode_cache`; fingerprint envelope (source hash, engine/PHP-target/format versions, opt level, features). `docs/performance/bytecode-cache.md`. | None as a cache; share *fingerprint concepts* only where safe. | (shared by P1) |
| Server script/include cache | partial | Process-local, intentionally not OPcache-equivalent. `docs/runtime/cache-architecture.md`. | Production-mode invalidation via engine-owned fingerprints. | P2 |
| Persistent type feedback | report-only / optional-default-off | `php_vm::persistent_feedback`; `--persistent-feedback-read`, `--persistent-feedback-stats-json`; `FPE-GAP-PERSISTENT-FEEDBACK`; `docs/research/persistent-type-feedback.md`; optional matrix row `phrust-persistent-feedback-optional`. Reader/validator only; accepted entries do not change execution. | (1) engine-owned **writer** with accept/reject-split stats done (P1); non-zero epoch capture remains. (2) **Quickening consumer already exists** (default-off, guard-protected, `seed_persistent_sites` reports installed count); the remaining P3 delta is **inline-cache** seeding + a dedicated `--persistent-feedback-consume` flag + per-entry seeded/dequickened attribution. | P1, P3 |
| Include/autoload dependency graph | partial | Request-local projection over ICs; `FPE-GAP-INCLUDE-AUTOLOAD-GRAPH`; `docs/research/include-autoload-dependency-graph.md`; counters `include_graph_hits/misses`, `autoload_graph_hits/misses`, `negative_lookup_hits`, `invalidations_by_reason`, `fallback_by_path_semantics`; `IncludePathFileFingerprint` (length, mtime nanos, readonly). Directory-version node **required but unused**; Composer map fingerprint slot **reserved/empty**; deployment-root fingerprint **absent**. | Directory-version abstraction (metadata+counters first), extended file identity, Composer classmap fingerprint slot, deployment-root fingerprint, new counters — all fail-closed. | P2 |
| Request-local arenas / persistent engine heap | partial | `FPE-GAP-REQUEST-ARENAS`; `docs/research/request-local-arenas.md`; counters `request_arena_allocations/bytes`, `request_pool_resets`, `persistent_engine_allocations/bytes`, `arena_fallback_allocations_by_reason`, `destructor_sensitive_arena_blocks`. Request-local frame/register pool is the implemented win; `persistent_engine_*` are now populated from the immutable-name interner footprint (`symbol_interner_footprint`, snapshot). | Extend the persistent heap owner to broader engine-only metadata (compiled-unit metadata handles, source-map metadata, symbol maps, validated feedback descriptors, fingerprints) beyond interned names. | P4 |
| Array fast paths / packed metadata | present | `FPE-GAP-ARRAY-FASTPATHS-V2`; runtime-owned packed metadata (element/key-kind/numeric-string/reference/COW/mutation-epoch/length); guarded packed fetch/append/foreach; family/fallback counter maps. | Full `array_sum/min/max` reductions, mutation-heavy, by-ref foreach — future. | (evidence for P5) |
| Record-like / mixed array shapes | present (guarded reads) | `FPE-GAP-RECORD-LIKE-ARRAY-SHAPES`; `docs/performance/array-shapes.md`; `FetchDim`/1-D `IssetDim` fail-closed helper reads for record/small-map; observed-shape + hit/miss + coercion/order/COW/reference fallback counters. **Recently extended**: interned record shapes + `StableKeyMap` (`c961c2ec`); dense `EmptyDim` now consumes the fail-closed shape helper (P5, behavior-preserving). | Write/foreach fast paths, immutable-literal storage, and native consumption remain evidence-gated. | P5 |
| Dense bytecode / managed fast profile | present | `FPE-GAP-DENSE-OBJECT-COVERAGE`; scalar/call/builtin/array/foreach + declared-slot property fetch/assign, method/static calls, constructors, includes, first-class callables; `bytecode_lowered_by_family`, `bytecode_executed_by_family`, unsupported/auto-fallback reason counters. **Recently extended**: cross-unit dense method dispatch + property-dim isset/empty probes (`c961c2ec`, `ef79cc2f`). | Property/method dense *metadata* surface for future tiers (P6); try/catch + generator/fiber dense families remain future. | P6 |
| Property assignment ICs | present | `FPE-GAP-PROPERTY-ASSIGNMENT-ICS`; guarded declared-property assignment ICs (class id, shape/layout epoch, slot, visibility, typed/readonly/init, reference-slot, hook/magic/dynamic, mutation epoch) + hit/miss + per-reason fallbacks; dense assignment executes through same metadata; property guards already surface in stencil/mid-tier reports. | (Property metadata already in reports.) | P6 (done) |
| Method call ICs | present | `FPE-GAP-METHOD-CALL-ICS`; capped polymorphic method-call IC targets + `dense_method_dispatch` counters; dense callers thread IC-resolved bodies. **P6**: method-dispatch guards + specific rejection reason now surface in mid-tier plan + copy-patch stencil reports. | Trivial getter/setter inline stays gated. | P6 (done) |
| Builtin intrinsics + SIMD byte kernels | present | `FPE-GAP-BUILTIN-INTRINSICS`, `FPE-GAP-SIMD-BYTE-KERNELS`; `docs/performance/builtin-intrinsics.md`, `docs/performance/simd-byte-kernels.md`; intrinsic ladder `strlen`, `count`, `is_int`, `is_string`, `is_array`, `strtolower`, `str_contains`, `str_starts_with`, `str_ends_with`. **Recently extended** (`c961c2ec`): `is_object`, `is_null`, `is_scalar`, `is_bool`, `is_float`, `current`, `key`, `array_keys`, `implode`. `php_source::byte_kernel` shared kernels. | At most **one** new candidate (e.g. `array_key_exists`, single-byte `explode`, default `htmlspecialchars`) with exact guards + stdlib differential fixtures. | P9 |
| Numeric-string classification cache | present | `FPE-GAP-NUMERIC-STRING-SPECIALIZATION`; `docs/performance/numeric-string-cache.md`; `php_runtime::numeric_string`; classify/hit/miss/specialization/overflow counters; guarded int/numeric-string add/sub/mul. | Persistent quickening opcodes for compare/array-key — future (feeds P3). | (feeds P3) |
| Superinstructions | present | `FPE-GAP-SUPERINSTRUCTIONS-V2`; `docs/performance/superinstructions.md`; mined corpus + selected fusion set (`load_const_echo`, `load_local_echo`, `binary_concat_echo`); candidate/emitted/executed/skipped/fallback counters; `php-vm dump-bytecode-patterns`. | Only evidence-backed new fusions with exact fallback accounting — not in this tranche. | — |
| Inline caches (fn/method/property/builtin/include) | present | Function/method/property/builtin/include-autoload IC surfaces + `inline-cache-smoke`. | Seed safely from persistent feedback (P3); avoid a second call/property semantics path. | P3 |
| Reference aliasing model | present (conservative) | `FPE-GAP-REFERENCE-ALIAS-DEOPT`; `php_vm::AliasState`; `docs/performance/reference-aliasing-deopt.md`; counters `frame_alias_state`, `alias_state_transitions`, `fast_path_disabled_by_reference`, `dequickened_by_reference`, `IC_invalidated_by_reference`, `dense_bytecode_fallback_by_reference`. Optimizing *through* references is disabled. | Improve alias summaries/markers (feeds P7). | (feeds P7) |
| Specialized call frames | present | `FPE-GAP-SPECIALIZED-CALL-FRAMES`; `docs/performance/specialized-call-frames.md`; counters `call_frame_layout_observed`, `tiny_frame_candidates`, `specialized_frame_hits`, `generic_frame_fallback_by_reason`, `arg_array_avoided`, `heap_frame_avoided`. Tiny leaf user fns avoid arg-snapshot clone. | Wider method/closure/variadic direct passing — future. | — |
| Hot/cold splitting | report-only | `FPE-GAP-HOT-COLD-SPLITTING`; `docs/performance/hot-cold-splitting.md`; aggregate `slow_path_calls_by_reason`. | Broader handler outlining with platform-stable evidence — future. | — |
| Optimizer profile guidance | present (guarded) | `FPE-GAP-OPTIMIZER-PROFILE-GUIDANCE`; per-pass verifier bracketing + rollback; dense jump-threading behind `--dense-jump-threading`. | Broader DFA/type folding stays gated on evidence. | — |
| Deopt / live-state / OSR metadata | report-only | `FPE-GAP-DEOPT-LIVE-STATE`; `php_vm::deopt`; `docs/performance/deopt-live-state-osr-metadata.md`; side-exit reason codes 1-7 + VM reasons; verified dense-region metadata; rejects try/finally, exceptions, generator/fiber, by-ref, include/eval before any resume guess. **P7**: per-slot `alias_class` markers (6-class `AliasState` model) + `reference_alias_summary` accessor + `alias_metadata_consistent` verifier, report-only. | Precise per-slot alias analysis (vs conservative per-region) remains future. | P7 (done) |
| Selective Cranelift regions | optional/default-off | `FPE-GAP-SELECTIVE-CRANELIFT-REGIONS`; `crates/php_jit`; `docs/performance/selective-cranelift-regions.md`, `docs/performance/cranelift/*`; eligibility/side-exit/blacklist/compile-budget counters; ADRs 0780-0787. Narrow packed/numeric regions only. | At most one new region family from positive counter evidence, with fixtures for each guard/side-exit. | P12 |
| Copy-and-patch stencils | report-only (no-exec) | `FPE-GAP-COPY-PATCH-STENCILS`; `docs/research/copy-and-patch-stencil-tier.md`; `php-vm dump-copy-patch-stencils --json`; `copy-patch-stencil-smoke`; code-size/patch-site/helper/unsupported/deopt/live-state/work-to-compile estimates. | Non-exec prereqs: helper ABI/status contract hash, code-cache key schema, W^X policy + non-exec test stubs, verifier fail-if-native rules. | P8 |
| Baseline native tier | report-only (no-exec) | `FPE-GAP-BASELINE-NATIVE-PREREQS`; ADR `0787`; `php-vm dump-baseline-native-stencil`; `baseline-native-stencil-smoke`. | Prereqs shared with copy-and-patch (P8); execution stays hard-blocked. | P8 |
| PHP-aware mid-tier | report-only | `FPE-GAP-PHP-MID-TIER`; `docs/research/php-mid-tier-compiler.md`; `php-vm dump-mid-tier-plan --json`; `mid-tier-plan-smoke`. | Feed better property/method dense metadata (P6); execution stays blocked. | P6 |
| Region profiling | report-only | `FPE-GAP-REGION-PROFILING`; `php-vm run --region-profile-json`; `docs/performance/region-profiling.md`; framework-smoke per-run summaries. | Executable region compilation stays blocked. | (feeds P11) |
| Real-workload representativeness | partial | `FPE-GAP-REAL-WORKLOAD-REPRESENTATIVENESS`, `FPE-GAP-COMPARATIVE-FASTEST-MATRIX`; `just fastest-engine-matrix`, `just framework-smoke`, WordPress root helpers; `docs/reference/performance-status.md`. | Workload-shape manifest + cross-shape counter-family comparison, no app-specific runtime behavior, no committed artifacts. | P11 |
| Server scalability visibility | partial | Server has in-flight limits, metrics, request tracing, script/include caches, and a blocking region (request-local PHP state is not `Send`). | Metrics for queue wait / worker saturation / cache-lock / per-worker cache hit-miss / rejection reason; backward-compatible perf-trace phases; structural-counter benchmark smoke. | P10 |
| VM-generator / meta-compiler | absent | Not productized. Existing VM/runtime/frontend stack with dense bytecode, counters, ICs, report-only native research. | Research note only mapping techniques to phrust-owned equivalents; **no rewrite**, no new bytecode format, no second semantic path. | P13 |

## Tempting duplicates — do NOT reimplement

- **A second parser / AST / semantic frontend / bytecode format / source-string
  execution path / stdlib / external-PHP path.** Hard architectural rule.
- **The trivial-predicate and array/string intrinsics already landed**
  (`is_object`, `is_null`, `is_scalar`, `is_bool`, `is_float`, `current`, `key`,
  `array_keys`, `implode`, plus the FPE-06/FPE-25 ladder). P9 must pick a *new*
  candidate (e.g. `array_key_exists`), not re-add these.
- **Cross-unit dense method dispatch and dense property-dim isset/empty probes**
  already landed (`c961c2ec`, `ef79cc2f`). P6 is about *exposing metadata to
  stencil/mid-tier reports*, not adding new dense execution.
- **`StableKeyMap` / interned record shapes** already landed in
  `crates/php_runtime/src/array.rs`. P5 must promote a *different* shape family
  or a genuinely new executable improvement, not redo shape interning wholesale.
- **The persistent-feedback reader/validator, metadata model, and
  fingerprint keying** already exist. P1 adds the *writer*; do not rewrite the
  validator or the metadata schema.
- **`IncludePathFileFingerprint` and the request-local include/autoload graph**
  already exist. P2 adds *fingerprint dimensions* (directory version, Composer
  map, deployment root); do not rebuild the graph.
- **Cranelift eligibility, side-exit, blacklist, and compile-budget machinery**
  already exist. P12 adds at most one region from evidence; do not build a
  generic function JIT.
- **Copy-and-patch, baseline-native, mid-tier, region-profiling, and
  deopt/live-state are report-only by policy.** P6/P7/P8 improve *metadata
  precision*; none of them may allocate executable memory or run native code.

## Smallest remaining deltas (pack order)

1. **P1** — persistent-feedback writer + engine-owned metadata cache, non-zero
   epochs, write/accept/reject stats. Consumption stays default-off.
2. **P2** — directory-version + file-identity + Composer + deployment-root
   fingerprints for the include/autoload graph, fail-closed.
3. **P3** — gated consumer that seeds quickening/ICs from accepted feedback;
   runtime guards still execute.
4. **P4** — persistent immutable engine heap owner so `persistent_engine_*`
   counters become meaningful for engine-only metadata.
5. **P5** — one array shape family promoted to an executable improvement.
6. **P6** — dense property/method shape metadata exposed to stencil/mid-tier
   reports (no native execution).
7. **P7** — one precise report-only deopt/live-state metadata slice.
8. **P8** — copy-and-patch non-execution prerequisites (ABI hash, code-cache key
   schema, W^X policy, verifier fail-if-native).
9. **P9** — one new exact builtin intrinsic with differential fixtures.
10. **P10** — server scalability measurement + topology guardrails.
11. **P11** — broader workload-shape evidence without app-specific behavior.
12. **P12** — one selective-Cranelift region only from positive counter
    evidence.
13. **P13** — VM-generator research note; never a rewrite trigger.

## Constraints reaffirmed

- No runtime behavior change closes a gap without source changes, focused
  fixtures, counters/reports, an off switch or conservative default, validation
  evidence, and optional-tool skip classification. Wall-clock-only evidence
  cannot close a gap.
- Baseline mode remains the correctness source of truth.
- Generated `target/performance/` evidence stays local and uncommitted.
