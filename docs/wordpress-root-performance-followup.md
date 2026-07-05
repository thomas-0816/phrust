# WordPress Root Performance Follow-Up

Date: 2026-07-06

Scope: real WordPress root page from a local WordPress 7.0 docroot, served by
this checkout through the Docker demo image (aarch64 Linux container,
`PHRUST_ENGINE_PRESET=experimental-jit`, script cache preloaded), measured
with `scripts/performance/wordpress_root_benchmark.py` against the running
container. The response body SHA-256 (`b748d4a1…`) is identical across every
measurement in this report.

## Result

| measurement | wall avg | value clones | array handle clones | rich fallback fns |
| --- | ---: | ---: | ---: | ---: |
| before (2026-07-05, e1912120) | 5218.8 ms | 98,980,010 | 27,038,140 | 359 |
| after clone-churn surgery | 1504.9 ms | 5,875,328 | 1,061,693 | 359 |
| after full recovery tranche | 1748.0 ms¹ | 5,833,364 | 1,054,137 | 81 |

¹ Same-day repeat runs vary ±15% with host load; the regression gate compares
against the recorded local baseline (1588.6 ms) and passes. The two "after"
rows are the same engine within noise.

Clone attribution is no longer blind: of the remaining 5.8M value clones,
2.2M are unattributed (was 93.4M), 1.5M are COW-separation contents, 1.0M are
register/local moves, and 0.4M are array element reads.

## What actually made it faster

Found by native sampling (`sample` on the release binary) driven from minimal
probes, not by counter dashboards:

1. **Whole-heap scans on value drops.** `release_unrooted_object_handles`
   deep-walked every PHP-visible root (cloning every array element) on every
   overwrite of an object-bearing value, to recycle object ids eagerly. On
   registry-heavy code that made each store O(heap). Object ids now recycle
   when their storage drops; destructor timing is unchanged.
2. **Quadratic record-shape growth.** Every new string key rebuilt and
   re-interned the entire key sequence. Shapes now grow through a weak
   interner with memoized transitions, and large growing maps switch to
   private shapes extended in place at O(1) per insert.
3. **Read-clone → separate → write-back property dims.**
   `$registry->items[$k] = $v` cloned the whole property array per write;
   `isset($registry->items[$k])` shared the handle and forced the next write
   to deep-copy. Property dim assignment now mutates object storage in
   place, and property/static dim isset/empty probe through borrowed walks.
   Registry-growth probe: 600 isset-guarded object adds 413 ms → 24 ms;
   20k adds 8.3 s → 0.22 s (linear).
4. **Dense `instanceof`.** The top dense fallback (272 of 354 instruction-
   subset fallbacks; every `wp_styles()`-style singleton guard) now executes
   densely through the same class-relation helper as the rich arm. Rich
   fallback function executions dropped 359 → 81.
5. **Return-value moves.** Return terminators move register operands out of
   the dying frame instead of cloning; pending finally blocks keep the
   cloning read. Fixing this surfaced and fixed a real semantic bug:
   returns previously ran only the innermost enclosing finally.

A latent correctness bug shipped in the previous tranche was also removed:
the LoadLocal/JumpIf branch collapse skipped the condition-register write
without liveness analysis, so any truthy `$value ?: $fallback` over a local
died with "read uninitialized register". The collapse is gone in both
interpreters and the regression is pinned by a fixture.

## Profiling is now safe and cheap

- `--request-profile` alone runs in Summary mode (phase timings only).
  VM counters and per-clone source attribution are explicit opt-ins
  (`--request-profile-vm-counters`, `--request-profile-source-attribution`),
  and `--request-profile-trigger-header` limits profiling to requests that
  send `x-phrust-request-profile: 1`.
- Source attribution uses fixed family ids into fixed arrays; a fully
  attributed root request now takes 2.2 s (was 13.6 s with the per-clone
  string/map recorder).
- `just profiler-overhead-gate` proves containment in one server process:
  unprofiled A = 0.7 ms, profiled B = 12.0 ms, unprofiled-after-profiled
  C = 0.7 ms on the fixture app.
- `just wordpress-root-regression-gate` compares a run against the recorded
  local baseline and fails on >5% latency / >10% clone-counter regressions;
  a missing WordPress environment is SKIP, never PASS.
- `just perf-pr-guard` fails performance branches that only ship docs,
  report scripts, counters, or metric renames.

## Honest residuals

- `call_user_func_array` moves values out of sole-owner argument arrays, but
  the owned path currently never triggers on the root request: builtin
  argument registers still pin a handle, so argument arrays always arrive
  shared. Unlocking it needs register-consumption liveness at lowering
  (same-block single-read analysis); until then CUFA argument clones remain
  (~209k call-argument snapshot clones per request). CUFA's own dispatch
  overhead is no longer a top cost (11 ms exclusive).
- 2.2M value clones remain unattributed; the next attribution targets are
  builtin bodies and constructor/default initialization.
- The persistent metadata layer stores quickening feedback templates only.
  Request-local state persistence is rejected per request and that rejection
  is again visible in
  `phrust_server_persistent_engine_rejected_persistence_total`.
- `vm_jit_compiled=0, vm_jit_executed=0`: the native tier still does not
  participate in this workload; no native claims are made.

## Reproduce

```bash
# benchmark against the running demo container
PHRUST_WORDPRESS_URL=http://127.0.0.1:18080 \
PHRUST_WORDPRESS_HOST_HEADER=127.0.0.1:8080 \
PHRUST_METRICS_TOKEN=dev-metrics-token \
nix develop -c just wordpress-root-benchmark

# record / enforce the local baseline
scripts/performance/wordpress_root_benchmark.py --record-baseline target/performance/wordpress-root/baseline.json
nix develop -c just wordpress-root-regression-gate

# attributed profile (opt-in, header-triggered profiling also available)
PHRUST_REQUEST_PROFILE=/var/tmp/phrust-profiles \
PHRUST_REQUEST_PROFILE_VM_COUNTERS=1 \
PHRUST_REQUEST_PROFILE_SOURCE_ATTRIBUTION=1 phrust-server ...
```
