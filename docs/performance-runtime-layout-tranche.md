# Runtime-Layout Performance Tranche

Summary of the runtime-layout performance tranche: value/string/array/object
storage layouts, direct call frames, builtin fast paths, dense call and
closure coverage, superinstructions, and the first record-shape native
region. Every fast path shares the semantic helpers of its generic path and
is pinned by differential fixtures against the reference engine on both
presets.

Regenerate the local evidence with:

```bash
nix develop -c just runtime-layout-performance-smoke   # counter gate + local ratchet
nix develop -c just app-flow-matrix                    # full ratio matrix (release + reference)
```

The smoke gate verifies stdout parity between the managed default and the
baseline compatibility preset before reporting any counters, asserts that
the tranche counters exist and fire on the scenarios that exercise them,
and diffs counter families against a local baseline
(`target/performance/runtime-layout/baseline.json`, refresh with
`--write-baseline`). Counter regressions are reported by default and
rejected when `PHRUST_RATCHET_ENFORCE=1`, keeping CI defaults non-flaky.

## What changed, by counter family

| Area | Change | Strongest local evidence |
| --- | --- | --- |
| Adaptive bookkeeping | Rich quickening observation gated to candidate kinds | rich observe attempts −71% |
| Value clones | Foreach steps in place, borrow-read operand arms, echo-before-store | `value_clones` −13% suite-wide at landing; collection later 94k → 66k |
| String storage | Interned `PhpString` with cached hash and symbol identity | string-hash cache 82–99% hit rates across flows |
| Call-name dispatch | Per-unit interned normalized name tables + symbol-guard ICs | zero uninterned-name fallbacks suite-wide |
| Object storage | Class-owned property layouts with declared slots + slot-backed property ICs | model hydration/container flows read/write via declared slots |
| Dense objects | Dense `NewObject` over shared instantiation helpers | zero dense-function fallbacks; 85% dense instruction share at landing |
| Array storage | Values-only packed storage; record (shaped) storage for string-key maps | array handle clones −34% on collection; mixed-hash lookups → 0 in config/translation/request/session |
| Foreach | Array-handle iteration with COW snapshot semantics | per-step element snapshots eliminated |
| Call frames | Argument-vector elision for bodies that never observe `func_get_args` | direct frames firing in all four call-heavy flows, zero fallbacks |
| Sort dispatch | One-time comparator resolution per sort | 1,140 direct comparator calls / 0 generic fallbacks on collection |
| String builtins | Exact intrinsics: `strtoupper`, scalar `str_replace`, default-flag `htmlspecialchars`, single-byte `explode` | template 225 / config 180 / hydration 75 / translation 180 intrinsic hits |
| `json_encode` | Default-flags direct encoder for packed/record arrays and scalars | hydration: 25 hits, 0 fallbacks; string allocations 706 → 406 |
| Array builtins | In-place dim writes (no forced COW separation), packed `array_slice`, count shape hits | collection separations 830 → 350 with 524 slot-fast map updates; session 225 map updates |
| Dense calls | Dense closures (`MakeClosure`), callable calls, callable acquisition; call-site strict-types on dense method dispatch; source-case autoloader names | closure-bearing programs fully dense; two correctness fixes pinned by fixtures |
| Superinstructions | `load_const`+`fetch_dim`, `load_local`+`load_const`, call+discard fusions | 365–1,900 fused dispatches retired per flow, byte-exact A/B |
| Native regions | Record-shape symbol-guarded lookup with guard table, side exits, interpreter resume | native hits with key-miss/layout/reference side exits resuming byte-exact |

## Current ratios vs reference PHP

See `docs/performance-app-flow-results.md` (regenerated only by
`just app-flow-matrix`) for the full per-scenario table with parity-checked
timings, compile/execute split, and the `Startup ms` column. Rows failing
stdout/stderr/status parity never report timings.

At the runtime-layout tranche close, the release fast preset measured a
**1.55x geomean** against reference PHP CLI across the ten app flows (down
from 1.64x at the previous tranche close), with every row parity-checked.

The effective web-app tranche that followed closed at a **1.30x geomean**
(cold-compile rows; the default-on bytecode cache reduces warm runs
further). Its structural wins: by-ref local arguments bind through caller
slots without materialized registers (session pins 225 -> 0), dense
callers thread method/static/constructor bodies through the request plan
(container 124/124 and hydration 150/150 dense bodies, zero rich
instructions), first-class callables and pipes lower dense, two new
constant fusions retire dispatches, and substr/trim/strict-in_array run
as exact intrinsic stubs (routing: 100 substr stub hits per request).
Remaining rows above 1.4x — `config_bootstrap_merge` (1.56x),
`session_auth_policy` (1.50x), `request_validation_errors` (1.49x),
`collection_transform_pagination` (1.48x) — share array/COW-dominated
inner loops: their top counters are `value_clones`,
`array_handle_clones`, and the residual `cow_separations` attributed by
`by_ref_arg_cow_separations` to genuine cross-variable sharing, plus
generic array builtins (`array_map`-style callbacks) that keep the
sort/comparator machinery hot. Those families are the next tranche's
targets.

## Remaining targets

The scenarios above 1.5x against reference PHP share three known costs, in
priority order. All three are `SUBSET_ALLOWED` under
`docs/performance-optimization-gates.md`: narrow guarded implementations that
reuse the generic semantic helpers and record fallback reasons are in scope
now.

1. **By-reference call arguments pin an array handle in the caller's
   argument register** for the whole callee, forcing one copy-on-write
   separation per call (session policy: 225/run). The lowering must stop
   materializing a by-ref argument as a register value; the null-placeholder
   argument encoding used for `is_callable` is the starting point, gated on
   callee-signature knowledge. (`SUBSET_ALLOWED`: by-ref argument location
   encoding for proven shapes, generic materialization as fallback.)

   Where the pin happens today: `lower_call_args_with_value_policy`
   (`php_ir/src/lower/expressions.rs`) lowers every non-placeholder argument
   through `lower_expr_to_register`, so a plain `$arr` argument becomes a
   `LoadLocal` into a register that stays live for the whole call while the
   `by_ref_local` metadata rides alongside. The binder
   (`php_vm/src/vm/arguments.rs`) then rebinds the parameter through
   `call_argument_reference_cell`, which never needed the materialized value
   for direct locals. The `by_ref_arg_*` counters attribute this per run:
   on `session_auth_policy`, 225/225 by-ref bindings are location-eligible,
   all currently materialized (`local_value_materialized`), all pinning an
   array handle, and all guaranteed one copy-on-write separation
   (`by_ref_arg_cow_separations`). Safe to replace: direct-local arguments
   to callees whose by-ref positions are provable at lowering time; the
   placeholder path already exists for quiet by-ref builtin arguments.
2. **Dense plans do not thread through method dispatch**: method bodies
   invoked from dense callers execute on the rich interpreter (container
   resolution: 41 rich method calls/run). Threading the dense plan through
   `execute_method_call_target` mirrors what the dense `CallFunction` arm
   already does for same-unit functions. (`SUBSET_ALLOWED`: IC-resolved
   concrete non-magic methods with verified dense bodies.)
3. **First-class callables and pipes stay rich-planned**
   (`ResolveCallable`, `Pipe`): acceptable local fallbacks today, next in
   line for dense coverage after the call-shape work above.
   (`SUBSET_ALLOWED`: stable guarded callable targets.)
